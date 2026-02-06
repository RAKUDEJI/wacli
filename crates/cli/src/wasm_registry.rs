use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use molt_registry_client::{
    RegistryEndpoint, WasmV1Client, WitRequest, auth_from_env, auth_from_header_line,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct WasmArgs {
    #[command(subcommand)]
    command: WasmCommands,
}

#[derive(Debug, Subcommand)]
enum WasmCommands {
    /// Fetch WIT (as text) for a component reference (tag or digest)
    Wit(WitArgs),

    /// Get indexed imports/exports for a component reference (tag or digest)
    Interfaces(InterfacesArgs),

    /// Get dependency info for a component reference (tag or digest)
    Dependencies(DependenciesArgs),

    /// Search components by imports/exports
    Search(SearchArgs),
}

#[derive(Debug, Clone, Parser)]
struct RegistryOpts {
    /// Base registry URL (e.g. https://registry.example.com)
    #[arg(long, value_name = "URL")]
    registry: Option<String>,

    /// Extra HTTP header (repeatable). Format: 'Header: value'
    #[arg(long, value_name = "HEADER")]
    header: Vec<String>,
}

#[derive(Debug, Parser)]
struct WitArgs {
    #[command(flatten)]
    registry: RegistryOpts,

    /// Repository name (may include '/')
    #[arg(value_name = "NAME")]
    name: String,

    /// Tag or manifest digest
    #[arg(value_name = "REFERENCE")]
    reference: String,

    /// Referrer artifactType filter
    #[arg(long, default_value = molt_registry_client::WIT_ARTIFACT_TYPE_V1)]
    artifact_type: String,

    /// Referrer package selector (matches manifest annotation dev.molt.wit.package)
    #[arg(long)]
    package: Option<String>,

    /// Write WIT text to a file instead of stdout
    #[arg(short, long, value_name = "FILE")]
    out: Option<PathBuf>,

    /// Print response metadata headers to stderr
    #[arg(long)]
    meta: bool,
}

#[derive(Debug, Parser)]
struct InterfacesArgs {
    #[command(flatten)]
    registry: RegistryOpts,

    /// Repository name (may include '/')
    #[arg(value_name = "NAME")]
    name: String,

    /// Tag or manifest digest
    #[arg(value_name = "REFERENCE")]
    reference: String,
}

#[derive(Debug, Parser)]
struct DependenciesArgs {
    #[command(flatten)]
    registry: RegistryOpts,

    /// Repository name (may include '/')
    #[arg(value_name = "NAME")]
    name: String,

    /// Tag or manifest digest
    #[arg(value_name = "REFERENCE")]
    reference: String,
}

#[derive(Debug, Parser)]
struct SearchArgs {
    #[command(flatten)]
    registry: RegistryOpts,

    /// Require these exported interfaces (repeatable, AND)
    #[arg(long = "export", value_name = "IFACE")]
    exports: Vec<String>,

    /// Require these imported interfaces (repeatable, AND)
    #[arg(long = "import", value_name = "IFACE")]
    imports: Vec<String>,

    /// Filter by OS (wasip1|wasip2)
    #[arg(long, value_name = "OS")]
    os: Option<String>,

    /// Page size (default 50, max 200)
    #[arg(long, value_name = "N")]
    limit: Option<u32>,

    /// Pagination cursor (base64url JSON token)
    #[arg(long)]
    cursor: Option<String>,
}

pub fn wasm(args: WasmArgs) -> Result<()> {
    match args.command {
        WasmCommands::Wit(args) => wit(args),
        WasmCommands::Interfaces(args) => interfaces(args),
        WasmCommands::Dependencies(args) => dependencies(args),
        WasmCommands::Search(args) => search(args),
    }
}

fn wit(args: WitArgs) -> Result<()> {
    let client = client_from_opts(&args.registry)?;
    let rt = runtime()?;

    let resp = rt.block_on(client.wit_text(
        &args.name,
        &args.reference,
        &WitRequest {
            artifact_type: Some(args.artifact_type),
            package: args.package,
        },
    ))?;

    if args.meta {
        if let Some(v) = resp.etag.as_deref() {
            eprintln!("ETag: {v}");
        }
        if let Some(v) = resp.subject_digest.as_deref() {
            eprintln!("OCI-Subject: {v}");
        }
        if let Some(v) = resp.referrer_manifest_digest.as_deref() {
            eprintln!("WIT-Referrer-Digest: {v}");
        }
    }

    if let Some(out) = args.out {
        fs::write(&out, resp.text.as_bytes())
            .with_context(|| format!("failed to write {}", out.display()))?;
    } else {
        print!("{}", resp.text);
    }
    Ok(())
}

fn interfaces(args: InterfacesArgs) -> Result<()> {
    let client = client_from_opts(&args.registry)?;
    let rt = runtime()?;

    let parsed = rt.block_on(client.interfaces(&args.name, &args.reference))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&parsed).unwrap_or_default()
    );
    Ok(())
}

fn dependencies(args: DependenciesArgs) -> Result<()> {
    let client = client_from_opts(&args.registry)?;
    let rt = runtime()?;

    let parsed = rt.block_on(client.dependencies(&args.name, &args.reference))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&parsed).unwrap_or_default()
    );
    Ok(())
}

fn search(args: SearchArgs) -> Result<()> {
    let client = client_from_opts(&args.registry)?;
    let rt = runtime()?;

    let q = molt_registry_client::SearchQuery {
        exports: args.exports,
        imports: args.imports,
        os: args.os,
        limit: args.limit,
        cursor: args.cursor,
    };

    let parsed = rt.block_on(client.search(&q))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&parsed).unwrap_or_default()
    );
    Ok(())
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to initialize async runtime")
}

fn client_from_opts(opts: &RegistryOpts) -> Result<WasmV1Client> {
    let base_url = match &opts.registry {
        Some(u) => u.clone(),
        None => {
            std::env::var("MOLT_REGISTRY").context("missing --registry (or set MOLT_REGISTRY)")?
        }
    };
    let endpoint = RegistryEndpoint::parse(&base_url)?;

    // Merge headers from CLI + env MOLT_AUTH_HEADER (for parity with previous behaviour).
    let mut header_lines = opts.header.clone();
    if let Ok(v) = std::env::var("MOLT_AUTH_HEADER") {
        if !v.trim().is_empty() {
            header_lines.push(v);
        }
    }

    // Determine auth:
    // - If any Authorization header is provided, use the last one.
    // - Otherwise fall back to USERNAME/PASSWORD env (or Anonymous).
    let auth = match last_authorization_header(&header_lines)? {
        Some(line) => auth_from_header_line(&line)?,
        None => auth_from_env()?,
    };

    // Extra headers: pass through non-Authorization headers.
    let extra_headers = header_map_without_authorization(&header_lines)?;
    let client = WasmV1Client::new_with_headers(endpoint, auth.clone(), extra_headers)?;
    Ok(client)
}

fn last_authorization_header(lines: &[String]) -> Result<Option<String>> {
    let mut out = None;
    for h in lines {
        if let Some((k, _)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("authorization") {
                out = Some(h.clone());
            }
        } else {
            bail!("invalid --header '{h}' (expected 'Header: value')");
        }
    }
    Ok(out)
}

fn header_map_without_authorization(lines: &[String]) -> Result<HeaderMap> {
    let mut out = HeaderMap::new();
    for h in lines {
        let (k, v) = h
            .split_once(':')
            .with_context(|| format!("invalid --header '{h}' (expected 'Header: value')"))?;
        if k.trim().eq_ignore_ascii_case("authorization") {
            continue;
        }
        let name = HeaderName::from_bytes(k.trim().as_bytes())
            .with_context(|| format!("invalid header name in --header '{h}'"))?;
        let value = HeaderValue::from_str(v.trim())
            .with_context(|| format!("invalid header value in --header '{h}'"))?;
        out.insert(name, value);
    }
    Ok(out)
}
