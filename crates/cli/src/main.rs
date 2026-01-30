mod check;
mod manifest;
mod wac_gen;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use indexmap::IndexMap;
use std::{
    collections::HashMap,
    fs,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};
use tracing_subscriber::{EnvFilter, fmt};
use wac_graph::{CompositionGraph, EncodeOptions};
use wac_parser::Document;
use wac_resolver::{FileSystemPackageResolver, packages};
use wac_types::{BorrowedPackageKey, Package};

use crate::manifest::Manifest;
use crate::wac_gen::generate_wac;

#[derive(Parser)]
#[command(name = "wacli")]
#[command(version, about = "WebAssembly Component composition CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new wacli project
    Init(InitArgs),

    /// Build CLI from wacli.json manifest
    Build(BuildArgs),

    /// Check component imports against an allowlist
    Check(CheckArgs),

    /// Compose WebAssembly components using a WAC source file
    Compose(ComposeArgs),

    /// Plug exports of components into imports of another component
    Plug(PlugArgs),
}

#[derive(Parser)]
struct InitArgs {
    /// Project directory (default: current directory)
    #[arg(value_name = "DIR")]
    dir: Option<PathBuf>,

    /// Package name
    #[arg(short, long, default_value = "example:my-cli")]
    name: String,
}

#[derive(Parser)]
struct BuildArgs {
    /// Path to wacli.json manifest
    #[arg(short, long, default_value = "wacli.json")]
    manifest: PathBuf,

    /// Output file path (overrides manifest)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Skip validation of the composed component
    #[arg(long)]
    no_validate: bool,

    /// Print generated WAC without composing
    #[arg(long)]
    print_wac: bool,
}

#[derive(Parser)]
struct ComposeArgs {
    /// The WAC source file
    #[arg(value_name = "FILE")]
    path: PathBuf,

    /// Output file path
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Directory to search for dependencies
    #[arg(long, default_value = "deps")]
    deps_dir: PathBuf,

    /// Specify dependency location: PKG=PATH
    #[arg(short = 'd', long = "dep", value_name = "PKG=PATH")]
    deps: Vec<String>,

    /// Skip validation of the composed component
    #[arg(long)]
    no_validate: bool,
}

#[derive(Parser)]
struct PlugArgs {
    /// The socket component (receives imports)
    #[arg(value_name = "SOCKET")]
    socket: PathBuf,

    /// Plug components (provide exports)
    #[arg(long = "plug", value_name = "FILE", required = true)]
    plugs: Vec<PathBuf>,

    /// Output file path
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Parser)]
struct CheckArgs {
    /// The WASM component to check
    #[arg(value_name = "FILE")]
    wasm: PathBuf,

    /// Path to wacli.json manifest (allowlist source)
    #[arg(short, long, default_value = "wacli.json", value_name = "FILE")]
    manifest: PathBuf,

    /// Output JSON report path
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Only output JSON (no human-readable output)
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            match cli.command {
                Commands::Init(args) => init(args),
                Commands::Build(args) => build(args).await,
                Commands::Check(args) => check_command(args),
                Commands::Compose(args) => compose(args).await,
                Commands::Plug(args) => plug(args),
            }
        })
}

fn init(args: InitArgs) -> Result<()> {
    let dir = args.dir.unwrap_or_else(|| PathBuf::from("."));

    // Create directory if it doesn't exist
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create directory: {}", dir.display()))?;

    let manifest_path = dir.join("wacli.json");

    if manifest_path.exists() {
        bail!("wacli.json already exists in {}", dir.display());
    }

    // Create default manifest
    let manifest = Manifest {
        package: manifest::Package {
            name: args.name,
            version: Some("0.1.0".to_string()),
        },
        ..Default::default()
    };

    let json_content = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, &json_content)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    eprintln!("Created: {}", manifest_path.display());
    eprintln!("\nNext steps:");
    eprintln!("  1. Edit wacli.json to configure your CLI");
    eprintln!("  2. Build your plugin components");
    eprintln!("  3. Run: wacli build");

    Ok(())
}

async fn build(args: BuildArgs) -> Result<()> {
    tracing::debug!("executing build command");

    // Read manifest
    let manifest_path = &args.manifest;
    let manifest = Manifest::from_file(manifest_path)?;

    // Get the directory containing the manifest for resolving relative paths
    let base_dir = manifest_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    // Generate WAC
    let wac_source = generate_wac(&manifest);

    if args.print_wac {
        println!("{}", wac_source);
        return Ok(());
    }

    // Build dependency map from manifest
    let mut deps: HashMap<String, PathBuf> = HashMap::new();

    // Add framework components
    deps.insert(
        "wacli-host".to_string(),
        base_dir.join(&manifest.framework.host),
    );
    deps.insert(
        "wacli-core".to_string(),
        base_dir.join(&manifest.framework.core),
    );
    deps.insert(
        "example:hello-registry".to_string(),
        base_dir.join(&manifest.framework.registry),
    );

    // Add command plugins
    for cmd in &manifest.command {
        deps.insert(cmd.package_name(), base_dir.join(&cmd.plugin));
    }

    // Parse WAC document
    let wac_path = PathBuf::from("<generated>");
    let document = Document::parse(&wac_source).map_err(|e| fmt_err(e, &wac_path))?;

    // Resolve packages
    let resolver = FileSystemPackageResolver::new(".", deps, false);
    let keys = packages(&document).map_err(|e| fmt_err(e, &wac_path))?;
    let resolved_packages: IndexMap<BorrowedPackageKey<'_>, Vec<u8>> = resolver.resolve(&keys)?;

    // Check for unresolved packages
    let mut missing: Vec<_> = keys
        .keys()
        .filter(|k| !resolved_packages.contains_key(*k))
        .collect();
    if !missing.is_empty() {
        missing.sort_by_key(|k| k.name);
        let names: Vec<_> = missing.iter().map(|k| k.name).collect();
        bail!("unresolved packages: {}", names.join(", "));
    }

    // Resolve the document
    let resolution = document
        .resolve(resolved_packages)
        .map_err(|e| fmt_err(e, &wac_path))?;

    // Encode the composition
    let bytes = resolution.encode(EncodeOptions {
        define_components: true,
        validate: !args.no_validate,
        ..Default::default()
    })?;

    // Determine output path
    let output_path = args
        .output
        .unwrap_or_else(|| base_dir.join(manifest.output_path()));

    // Create output directory if needed
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Write output
    fs::write(&output_path, &bytes)
        .with_context(|| format!("failed to write output file: {}", output_path.display()))?;

    eprintln!("Built: {}", output_path.display());

    Ok(())
}

fn fmt_err(e: impl std::fmt::Display, path: &Path) -> anyhow::Error {
    anyhow::Error::msg(format!("{}: {}", path.display(), e))
}

fn parse_dep(s: &str) -> Result<(String, PathBuf)> {
    let (k, v) = s
        .split_once('=')
        .context("dependency format should be PKG=PATH")?;
    Ok((k.trim().to_string(), PathBuf::from(v.trim())))
}

async fn compose(args: ComposeArgs) -> Result<()> {
    tracing::debug!("executing compose command");

    // Read the WAC source file
    let contents = fs::read_to_string(&args.path)
        .with_context(|| format!("failed to read file `{}`", args.path.display()))?;

    // Parse the document
    let document = Document::parse(&contents).map_err(|e| fmt_err(e, &args.path))?;

    // Parse dependency overrides
    let overrides: HashMap<String, PathBuf> = args
        .deps
        .iter()
        .map(|s| parse_dep(s))
        .collect::<Result<_>>()?;

    // Resolve packages
    let resolver = FileSystemPackageResolver::new(&args.deps_dir, overrides, false);
    let keys = packages(&document).map_err(|e| fmt_err(e, &args.path))?;
    let resolved_packages: IndexMap<BorrowedPackageKey<'_>, Vec<u8>> = resolver.resolve(&keys)?;

    // Check for unresolved packages
    let mut missing: Vec<_> = keys
        .keys()
        .filter(|k| !resolved_packages.contains_key(*k))
        .collect();
    if !missing.is_empty() {
        missing.sort_by_key(|k| k.name);
        let names: Vec<_> = missing.iter().map(|k| k.name).collect();
        bail!(
            "unresolved packages: {}. Use --dep or place in deps directory.",
            names.join(", ")
        );
    }

    // Resolve the document
    let resolution = document
        .resolve(resolved_packages)
        .map_err(|e| fmt_err(e, &args.path))?;

    // Check output
    if args.output.is_none() && std::io::stdout().is_terminal() {
        bail!("cannot print binary wasm output to terminal; use -o to specify output file");
    }

    // Encode the composition
    let bytes = resolution.encode(EncodeOptions {
        define_components: true,
        validate: !args.no_validate,
        ..Default::default()
    })?;

    // Write output
    match args.output {
        Some(path) => {
            fs::write(&path, &bytes)
                .with_context(|| format!("failed to write output file `{}`", path.display()))?;
            eprintln!("Composed: {}", path.display());
        }
        None => {
            std::io::stdout()
                .write_all(&bytes)
                .context("failed to write to stdout")?;
        }
    }

    Ok(())
}

fn plug(args: PlugArgs) -> Result<()> {
    tracing::debug!("executing plug command");

    let mut graph = CompositionGraph::new();

    // Load socket component
    let socket_bytes = fs::read(&args.socket)
        .with_context(|| format!("failed to read socket `{}`", args.socket.display()))?;
    let socket_pkg = Package::from_bytes("socket", None, socket_bytes, graph.types_mut())?;
    let socket = graph.register_package(socket_pkg)?;

    // Load plug components
    let mut plug_ids = Vec::new();
    for (i, plug_path) in args.plugs.iter().enumerate() {
        let plug_bytes = fs::read(plug_path)
            .with_context(|| format!("failed to read plug `{}`", plug_path.display()))?;
        let name = format!("plug{}", i);
        let plug_pkg = Package::from_bytes(&name, None, plug_bytes, graph.types_mut())?;
        let plug_id = graph.register_package(plug_pkg)?;
        plug_ids.push(plug_id);
    }

    // Plug them together
    wac_graph::plug(&mut graph, plug_ids, socket)?;

    // Encode
    let bytes = graph.encode(EncodeOptions::default())?;

    // Check output
    if args.output.is_none() && std::io::stdout().is_terminal() {
        bail!("cannot print binary wasm output to terminal; use -o to specify output file");
    }

    // Write output
    match args.output {
        Some(path) => {
            fs::write(&path, &bytes)
                .with_context(|| format!("failed to write output file `{}`", path.display()))?;
            eprintln!("Plugged: {}", path.display());
        }
        None => {
            std::io::stdout()
                .write_all(&bytes)
                .context("failed to write to stdout")?;
        }
    }

    Ok(())
}

fn check_command(args: CheckArgs) -> Result<()> {
    tracing::debug!("executing check command");

    let manifest = Manifest::from_file(&args.manifest)?;
    let report = check::check_imports(&args.wasm, &manifest.allowlist, &args.manifest)?;

    // Output JSON report if requested
    if let Some(output_path) = &args.output {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(&report)?;
        fs::write(output_path, &json)
            .with_context(|| format!("failed to write report: {}", output_path.display()))?;
        if !args.json {
            eprintln!("Report: {}", output_path.display());
        }
    }

    if args.json {
        // JSON-only output
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        // Human-readable output
        eprintln!();
        eprintln!("=== Import Check Results ===");
        eprintln!("Artifact: {}", report.artifact);
        eprintln!("Total imports: {}", report.imports.len());

        if !report.extra_imports.is_empty() {
            eprintln!();
            eprintln!(
                "WARNING: Found {} import(s) NOT in allowlist:",
                report.extra_imports.len()
            );
            for imp in &report.extra_imports {
                eprintln!("  - {}", imp);
            }
            eprintln!();
            bail!("Component has imports outside the allowed set");
        } else {
            eprintln!("OK: All imports are within the allowlist");
            if !report.missing_imports.is_empty() {
                eprintln!(
                    "Note: {} allowed import(s) not used",
                    report.missing_imports.len()
                );
            }
        }
    }

    // Check for extra imports (fail if any)
    if !report.extra_imports.is_empty() && args.json {
        std::process::exit(1);
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
