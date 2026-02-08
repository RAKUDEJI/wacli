mod command_metadata;
mod component_scan;
mod lock;
mod manifest;
mod registry_gen_wat;
mod registry_pull;
mod wac_gen;
mod wasm_registry;
mod wit;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use indexmap::IndexMap;
use self_update::{Status, backends::github::Update};
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

use crate::component_scan::{scan_commands, scan_commands_optional};
use crate::registry_gen_wat::{AppMeta, generate_registry_wat, get_prebuilt_registry};
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

    /// Build CLI from defaults/ and commands/ directories
    Build(BuildArgs),

    /// Compose WebAssembly components using a WAC source file
    Compose(ComposeArgs),

    /// Plug exports of components into imports of another component
    Plug(PlugArgs),

    /// Molt WASM-aware registry helper commands (/wasm/v1)
    Wasm(wasm_registry::WasmArgs),

    #[cfg(feature = "runtime")]
    /// Run a composed CLI component with dynamic pipes
    Run(RunArgs),

    /// Update wacli from GitHub Releases
    SelfUpdate(SelfUpdateArgs),
}

#[derive(Parser)]
struct BuildArgs {
    /// Path to a wacli manifest (defaults to ./wacli.json if present)
    #[arg(long, value_name = "FILE")]
    manifest: Option<PathBuf>,

    /// Package name (e.g., "example:my-cli") [default: example:my-cli]
    #[arg(long)]
    name: Option<String>,

    /// Package version [default: 0.1.0]
    #[arg(long)]
    version: Option<String>,

    /// Package description used for global help output
    #[arg(long)]
    description: Option<String>,

    /// Output file path [default: my-cli.component.wasm]
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Defaults directory containing framework components [default: defaults]
    #[arg(long = "defaults-dir", value_name = "DIR")]
    defaults_dir: Option<PathBuf>,

    /// Commands directory containing plugin components [default: commands]
    #[arg(long = "commands-dir", value_name = "DIR")]
    commands_dir: Option<PathBuf>,

    /// Skip validation of the composed component
    #[arg(long)]
    no_validate: bool,

    /// Print generated WAC without composing
    #[arg(long)]
    print_wac: bool,

    /// Use `defaults/registry.component.wasm` instead of generating a registry
    ///
    /// By default, wacli generates a fresh registry component on every build.
    #[arg(long)]
    use_prebuilt_registry: bool,

    /// Update `wacli.lock` by resolving registry tags to digests (requires MOLT_REGISTRY).
    ///
    /// Without this flag, wacli will prefer digests already pinned in `wacli.lock`.
    #[arg(long)]
    update_lock: bool,
}

#[derive(Parser)]
struct InitArgs {
    /// Project directory (default: current directory)
    #[arg(value_name = "DIR")]
    dir: Option<PathBuf>,

    /// Download framework components (host/core) into defaults/ (requires MOLT_REGISTRY)
    #[arg(long)]
    with_components: bool,

    /// Overwrite existing component and WIT files when initializing
    #[arg(long)]
    overwrite: bool,
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

#[cfg(feature = "runtime")]
#[derive(Parser)]
struct RunArgs {
    /// Composed CLI component (.component.wasm)
    #[arg(value_name = "COMPONENT")]
    component: PathBuf,

    /// Preopen a directory (HOST[::GUEST], repeatable)
    #[arg(long = "dir", value_name = "HOST[::GUEST]")]
    dirs: Vec<String>,

    /// Arguments passed to the command
    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(Parser)]
struct SelfUpdateArgs {
    /// Update to a specific version (e.g., 0.0.14). Defaults to latest.
    #[arg(long)]
    version: Option<String>,
}

fn main() {
    // Best-effort: load `.env` from the current working directory for local/dev usage.
    // This is a no-op if the file is missing.
    let _ = dotenvy::dotenv();

    init_tracing();
    let cli = Cli::parse();

    if let Err(err) = dispatch(cli) {
        report_error(err);
        std::process::exit(1);
    }
}

fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init(args) => init(args),
        Commands::Build(args) => build(args),
        Commands::Compose(args) => compose(args),
        Commands::Plug(args) => plug(args),
        Commands::Wasm(args) => wasm_registry::wasm(args),
        #[cfg(feature = "runtime")]
        Commands::Run(args) => run(args),
        Commands::SelfUpdate(args) => self_update(args),
    }
}

fn report_error(err: anyhow::Error) {
    if is_component_interface_mismatch(&err) {
        eprintln!("Error: Component interface mismatch\n");
        eprintln!("This usually happens when:");
        eprintln!("1. wacli and wacli-cdk versions don't match");
        eprintln!("2. Old component files remain in commands/\n");
        eprintln!("Solutions:");
        eprintln!("- Update: wacli self-update && update wacli-cdk in Cargo.toml");
        eprintln!("- Clean: rm commands/**/*.component.wasm && rebuild");
        eprintln!("- Verify: wacli --version && rg wacli-cdk commands/*/Cargo.toml\n");
        eprintln!("Details: {err}");
    } else {
        eprintln!("Error: {err}");
    }
}

fn is_component_interface_mismatch(err: &anyhow::Error) -> bool {
    let needles = [
        "component has no import named",
        "missing import",
        "unknown import",
        "no import named",
    ];
    for cause in err.chain() {
        let msg = cause.to_string().to_lowercase();
        if msg.contains("wacli:cli/") && needles.iter().any(|n| msg.contains(n)) {
            return true;
        }
    }
    false
}

fn init(args: InitArgs) -> Result<()> {
    let dir = args.dir.unwrap_or_else(|| PathBuf::from("."));

    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create directory: {}", dir.display()))?;

    let defaults_dir = dir.join("defaults");
    let commands_dir = dir.join("commands");

    fs::create_dir_all(&defaults_dir)
        .with_context(|| format!("failed to create directory: {}", defaults_dir.display()))?;
    fs::create_dir_all(&commands_dir)
        .with_context(|| format!("failed to create directory: {}", commands_dir.display()))?;

    write_plugin_wit(&dir, args.overwrite)?;

    if args.with_components {
        download_framework_components(&dir, &defaults_dir, args.overwrite)?;
    }

    manifest::write_default_manifest(&dir, args.overwrite)?;

    eprintln!("Created:");
    eprintln!("  {}", defaults_dir.display());
    eprintln!("  {}", commands_dir.display());
    eprintln!("  {}", dir.join("wit").display());
    eprintln!("  {}", dir.join(manifest::DEFAULT_MANIFEST_NAME).display());
    eprintln!();
    eprintln!("Next steps:");
    if args.with_components {
        eprintln!("  1. Place your command components in commands/");
        eprintln!("  2. Run: wacli build");
    } else {
        eprintln!(
            "  1. Place host.component.wasm and core.component.wasm in defaults/ (or set MOLT_REGISTRY to pull them)"
        );
        eprintln!(
            "  2. Place your command components in commands/ (or set build.commands in wacli.json to pull them)"
        );
        eprintln!("  3. Run: wacli build");
    }

    Ok(())
}

const PLUGIN_WIT: &str = wit::COMMAND_WIT;
const TYPES_WIT: &str = wit::TYPES_WIT;
const HOST_ENV_WIT: &str = wit::HOST_ENV_WIT;
const HOST_IO_WIT: &str = wit::HOST_IO_WIT;
const HOST_FS_WIT: &str = wit::HOST_FS_WIT;
const HOST_PROCESS_WIT: &str = wit::HOST_PROCESS_WIT;
const HOST_PIPES_WIT: &str = wit::HOST_PIPES_WIT;
const PIPE_RUNTIME_WIT: &str = wit::PIPE_RUNTIME_WIT;
const PIPE_WIT: &str = wit::PIPE_WIT;
const SCHEMA_WIT: &str = wit::SCHEMA_WIT;
const REGISTRY_SCHEMA_WIT: &str = wit::REGISTRY_SCHEMA_WIT;

fn write_plugin_wit(project_dir: &Path, overwrite: bool) -> Result<()> {
    let wit_dir = project_dir.join("wit");
    fs::create_dir_all(&wit_dir)
        .with_context(|| format!("failed to create directory: {}", wit_dir.display()))?;

    let files = [
        ("types.wit", TYPES_WIT),
        ("host-env.wit", HOST_ENV_WIT),
        ("host-io.wit", HOST_IO_WIT),
        ("host-fs.wit", HOST_FS_WIT),
        ("host-process.wit", HOST_PROCESS_WIT),
        ("host-pipes.wit", HOST_PIPES_WIT),
        ("pipe-runtime.wit", PIPE_RUNTIME_WIT),
        ("schema.wit", SCHEMA_WIT),
        ("registry-schema.wit", REGISTRY_SCHEMA_WIT),
        ("command.wit", PLUGIN_WIT),
        ("pipe.wit", PIPE_WIT),
    ];

    for (name, contents) in files {
        write_wit_file(&wit_dir, name, contents, overwrite)?;
    }
    Ok(())
}

fn write_wit_file(dir: &Path, name: &str, contents: &str, overwrite: bool) -> Result<()> {
    let dest = dir.join(name);
    if dest.exists() && !overwrite {
        tracing::info!("{name} already exists, skipping");
        return Ok(());
    }

    let tmp_path = dest.with_extension("tmp");
    fs::write(&tmp_path, contents)
        .with_context(|| format!("failed to write {}", tmp_path.display()))?;
    if overwrite && dest.exists() {
        fs::remove_file(&dest).with_context(|| format!("failed to remove {}", dest.display()))?;
    }
    fs::rename(&tmp_path, &dest)
        .with_context(|| format!("failed to move {} into place", dest.display()))?;
    tracing::info!("installed {} -> {}", name, dest.display());
    Ok(())
}

fn download_framework_components(
    project_dir: &Path,
    defaults_dir: &Path,
    overwrite: bool,
) -> Result<()> {
    let host_path = defaults_dir.join("host.component.wasm");
    let core_path = defaults_dir.join("core.component.wasm");

    let needs_host = overwrite || !host_path.exists();
    let needs_core = overwrite || !core_path.exists();
    if !needs_host && !needs_core {
        return Ok(());
    }

    let Some(client) = molt_registry_client::OciWasmClient::from_env()? else {
        bail!(
            "MOLT_REGISTRY is not configured.\n\n\
Set MOLT_REGISTRY (and auth) to download framework components, or omit --with-components and provide defaults/host.component.wasm + defaults/core.component.wasm manually."
        );
    };

    let lock_path = crate::lock::lock_path(project_dir);
    let mut lock = crate::lock::load_lock(&lock_path)?.unwrap_or_default();
    let mut lock_dirty = false;

    let version_tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    let host_repo = std::env::var("WACLI_HOST_REPO").unwrap_or_else(|_| "wacli/host".to_string());
    let core_repo = std::env::var("WACLI_CORE_REPO").unwrap_or_else(|_| "wacli/core".to_string());
    let host_ref = std::env::var("WACLI_HOST_REFERENCE").unwrap_or_else(|_| version_tag.clone());
    let core_ref = std::env::var("WACLI_CORE_REFERENCE").unwrap_or(version_tag);

    tracing::info!(
        "downloading framework components from registry {}",
        std::env::var("MOLT_REGISTRY").unwrap_or_default()
    );

    if !needs_host {
        tracing::info!("host.component.wasm already exists, skipping download");
    } else {
        let pulled = crate::registry_pull::pull_component_wasm_to_file_with_digests(
            &client, &host_repo, &host_ref, &host_path, overwrite,
        )
        .context("failed to pull host.component.wasm from registry")?;
        if let Some(pulled) = pulled {
            lock.set_framework_host(crate::lock::LockedComponent {
                repo: host_repo.clone(),
                reference: host_ref.clone(),
                digest: pulled.manifest_digest,
                layer_digest: Some(pulled.layer_digest),
            });
            lock_dirty = true;
        }
        tracing::info!("downloaded host.component.wasm -> {}", host_path.display());
    }

    if !needs_core {
        tracing::info!("core.component.wasm already exists, skipping download");
    } else {
        let pulled = crate::registry_pull::pull_component_wasm_to_file_with_digests(
            &client, &core_repo, &core_ref, &core_path, overwrite,
        )
        .context("failed to pull core.component.wasm from registry")?;
        if let Some(pulled) = pulled {
            lock.set_framework_core(crate::lock::LockedComponent {
                repo: core_repo.clone(),
                reference: core_ref.clone(),
                digest: pulled.manifest_digest,
                layer_digest: Some(pulled.layer_digest),
            });
            lock_dirty = true;
        }
        tracing::info!("downloaded core.component.wasm -> {}", core_path.display());
    }

    if lock_dirty {
        if let Ok(registry) = std::env::var("MOLT_REGISTRY") {
            let v = registry.trim();
            if !v.is_empty() {
                lock.molt_registry = Some(v.to_string());
            }
        }
        crate::lock::write_lock(&lock_path, &mut lock)
            .with_context(|| format!("failed to write lock file: {}", lock_path.display()))?;
        tracing::info!("updated lock file: {}", lock_path.display());
    }

    Ok(())
}

fn build(args: BuildArgs) -> Result<()> {
    tracing::debug!("executing build command");

    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let loaded = manifest::load_manifest(args.manifest.as_deref())?;
    let base_dir = loaded
        .as_ref()
        .map(|m| m.base_dir.as_path())
        .unwrap_or(cwd.as_path());
    let m_build = loaded.as_ref().and_then(|m| m.manifest.build.as_ref());

    let name = args
        .name
        .or_else(|| m_build.and_then(|m| m.name.clone()))
        .unwrap_or_else(|| "example:my-cli".to_string());

    let version = args
        .version
        .or_else(|| m_build.and_then(|m| m.version.clone()))
        .unwrap_or_else(|| "0.1.0".to_string());

    let description = args
        .description
        .or_else(|| m_build.and_then(|m| m.description.clone()))
        .unwrap_or_default();

    // If the user already provided a version in `--name`, don't append another one.
    let package_name = if name.contains('@') {
        name.clone()
    } else {
        format!("{name}@{version}")
    };

    let (app_name, app_version) = package_name
        .split_once('@')
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .unwrap_or((package_name.clone(), String::new()));
    let app_meta = AppMeta {
        name: app_name,
        version: app_version,
        description,
    };

    #[derive(Clone, Copy)]
    enum PathOrigin {
        Cli,
        Manifest,
        Default,
    }

    let resolve_path = |origin: PathOrigin, p: PathBuf| -> PathBuf {
        if p.is_absolute() {
            return p;
        }
        match origin {
            PathOrigin::Cli => cwd.join(p),
            PathOrigin::Manifest | PathOrigin::Default => base_dir.join(p),
        }
    };

    let (defaults_raw, defaults_origin) = match args.defaults_dir {
        Some(p) => (p, PathOrigin::Cli),
        None => match m_build.and_then(|m| m.defaults_dir.clone()) {
            Some(p) => (p, PathOrigin::Manifest),
            None => (PathBuf::from("defaults"), PathOrigin::Default),
        },
    };
    let defaults_dir = resolve_path(defaults_origin, defaults_raw);

    let (commands_raw, commands_origin) = match args.commands_dir {
        Some(p) => (p, PathOrigin::Cli),
        None => match m_build.and_then(|m| m.commands_dir.clone()) {
            Some(p) => (p, PathOrigin::Manifest),
            None => (PathBuf::from("commands"), PathOrigin::Default),
        },
    };
    let commands_dir = resolve_path(commands_origin, commands_raw);

    let (output_raw, output_origin) = match args.output {
        Some(p) => (p, PathOrigin::Cli),
        None => match m_build.and_then(|m| m.output.clone()) {
            Some(p) => (p, PathOrigin::Manifest),
            None => (PathBuf::from("my-cli.component.wasm"), PathOrigin::Default),
        },
    };
    let output_path = resolve_path(output_origin, output_raw);

    // Lock file (digest pinning for registry pulls).
    let lock_path = crate::lock::lock_path(base_dir);
    let mut lock = crate::lock::load_lock(&lock_path)?.unwrap_or_default();
    let mut lock_dirty = false;

    // Resolve framework components (host/core).
    //
    // Prefer local defaults/, but fall back to pulling from an OCI registry if configured.
    let (host_path, core_path) = resolve_framework_components(
        &defaults_dir,
        base_dir,
        args.update_lock,
        &mut lock,
        &mut lock_dirty,
    )?;

    // Resolve command plugins (local + optional registry sources).
    let registry_commands = m_build.and_then(|m| m.commands.clone()).unwrap_or_default();

    let mut commands = if registry_commands.is_empty() {
        scan_commands(&commands_dir)?
    } else {
        scan_commands_optional(&commands_dir)?
    };

    let mut registry_resolved = resolve_registry_commands(
        base_dir,
        &registry_commands,
        args.update_lock,
        &mut lock,
        &mut lock_dirty,
    )
    .context("failed to resolve registry commands")?;
    commands.append(&mut registry_resolved);

    // Enforce global uniqueness and deterministic ordering.
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    let mut seen: HashMap<String, PathBuf> = HashMap::new();
    for cmd in &commands {
        if let Some(prev) = seen.insert(cmd.name.clone(), cmd.path.clone()) {
            bail!(
                "duplicate command name '{}':\n  {}\n  {}",
                cmd.name,
                prev.display(),
                cmd.path.display()
            );
        }
    }

    if commands.is_empty() {
        bail!("no commands configured (commandsDir empty/missing, and build.commands is not set)");
    }

    tracing::info!("found {} command(s)", commands.len());
    for cmd in &commands {
        tracing::debug!("  - {}: {}", cmd.name, cmd.path.display());
    }

    // Get registry component.
    //
    // By default, generate a fresh registry on every build and keep the build
    // artifact out of defaults/. Use a pre-built defaults/registry.component.wasm
    // only with --use-prebuilt-registry.
    let registry_path = if args.use_prebuilt_registry {
        get_prebuilt_registry(&defaults_dir)
            .context("defaults/registry.component.wasm not found (remove --use-prebuilt-registry or add the file)")?
    } else {
        // Generate registry component on every build. Keep build artifacts out of defaults/.
        tracing::info!("generating registry component...");
        tracing::info!("using WAT template registry generator");
        let registry_bytes = generate_registry_wat(&commands, &app_meta)
            .context("failed to generate registry (WAT)")?;

        // Write to a local build cache directory.
        let cache_dir = base_dir.join(".wacli");
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("failed to create directory: {}", cache_dir.display()))?;
        let generated_path = cache_dir.join("registry.component.wasm");
        fs::write(&generated_path, &registry_bytes)
            .context("failed to write generated registry")?;
        tracing::info!("generated: {}", generated_path.display());

        generated_path
    };

    // Generate WAC
    let wac_source = generate_wac(&package_name, &commands);

    if args.print_wac {
        println!("{}", wac_source);
        return Ok(());
    }

    // Build dependency map
    let mut deps: HashMap<String, PathBuf> = HashMap::new();

    // Add framework components
    deps.insert("wacli:host".to_string(), host_path);
    deps.insert("wacli:core".to_string(), core_path);
    deps.insert("wacli:registry".to_string(), registry_path);

    // Add command plugins
    for cmd in &commands {
        deps.insert(cmd.package_name(), cmd.path.clone());
    }

    // Parse WAC document
    let wac_path = PathBuf::from("<generated>");
    let document = Document::parse(&wac_source).map_err(|e| fmt_err(e, &wac_path))?;

    // Resolve packages
    let resolver = FileSystemPackageResolver::new(base_dir, deps, false);
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

    // Create output directory if needed
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Write output
    fs::write(&output_path, &bytes)
        .with_context(|| format!("failed to write output file: {}", output_path.display()))?;

    if lock_dirty {
        if let Ok(registry) = std::env::var("MOLT_REGISTRY") {
            let v = registry.trim();
            if !v.is_empty() {
                lock.molt_registry = Some(v.to_string());
            }
        }
        crate::lock::write_lock(&lock_path, &mut lock)
            .with_context(|| format!("failed to write lock file: {}", lock_path.display()))?;
        tracing::info!("updated lock file: {}", lock_path.display());
    }

    eprintln!("Built: {}", output_path.display());

    Ok(())
}

fn resolve_framework_components(
    defaults_dir: &Path,
    base_dir: &Path,
    update_lock: bool,
    lock: &mut crate::lock::LockFile,
    lock_dirty: &mut bool,
) -> Result<(PathBuf, PathBuf)> {
    let host_local = defaults_dir.join("host.component.wasm");
    let core_local = defaults_dir.join("core.component.wasm");

    // Preserve defaults/ semantics: if both are present locally, prefer them.
    if host_local.exists() && core_local.exists() {
        return Ok((host_local, core_local));
    }

    let client = molt_registry_client::OciWasmClient::from_env()?;

    let version_tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    let desired_host_repo =
        std::env::var("WACLI_HOST_REPO").unwrap_or_else(|_| "wacli/host".to_string());
    let desired_core_repo =
        std::env::var("WACLI_CORE_REPO").unwrap_or_else(|_| "wacli/core".to_string());
    let desired_host_ref =
        std::env::var("WACLI_HOST_REFERENCE").unwrap_or_else(|_| version_tag.clone());
    let desired_core_ref = std::env::var("WACLI_CORE_REFERENCE").unwrap_or(version_tag);

    let cache_dir = base_dir.join(".wacli").join("framework");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create directory: {}", cache_dir.display()))?;

    let cache_path = |repo: &str, digest: &str, file_name: &str| -> PathBuf {
        cache_dir
            .join(molt_registry_client::sanitize_path_segment(repo))
            .join(molt_registry_client::sanitize_path_segment(digest))
            .join(file_name)
    };

    // Resolve host/core individually. Prefer local defaults if present; otherwise use lock+cache,
    // then fall back to resolving from the registry.
    let host_path = if host_local.exists() {
        host_local
    } else if !update_lock {
        if let Some(locked) = lock.framework_host() {
            let digest = locked.digest.trim();
            if digest.is_empty() {
                bail!("wacli.lock has an empty digest for framework host");
            }

            let dest = cache_path(&locked.repo, digest, "host.component.wasm");
            if dest.exists() {
                tracing::info!("using cached host (locked): {}", dest.display());
                dest
            } else {
                let Some(client) = client.as_ref() else {
                    bail!(
                        "framework host is not available locally, and MOLT_REGISTRY is not configured.\n\n\
Expected cached host at: {}\n\n\
Set MOLT_REGISTRY (and auth) to download framework components.",
                        dest.display()
                    );
                };
                tracing::info!(
                    "defaults host.component.wasm missing; pulling locked digest from registry: {}@{}",
                    locked.repo,
                    digest
                );
                crate::registry_pull::pull_component_wasm_to_file(
                    client,
                    &locked.repo,
                    digest,
                    &dest,
                    false,
                )
                .context("failed to pull host.component.wasm from registry (locked)")?;
                tracing::info!("cached host: {}", dest.display());
                dest
            }
        } else {
            // No lock entry; resolve via registry.
            let Some(client) = client.as_ref() else {
                return crate::component_scan::verify_defaults(defaults_dir);
            };

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to initialize async runtime")?;
            let (manifest_digest, layer_digest) = rt
                .block_on(client.resolve_component_digests(&desired_host_repo, &desired_host_ref))
                .with_context(|| {
                    format!(
                        "failed to resolve host digest from registry: {}:{}",
                        desired_host_repo, desired_host_ref
                    )
                })?;

            lock.set_framework_host(crate::lock::LockedComponent {
                repo: desired_host_repo.clone(),
                reference: desired_host_ref.clone(),
                digest: manifest_digest.clone(),
                layer_digest: Some(layer_digest),
            });
            *lock_dirty = true;

            let dest = cache_path(&desired_host_repo, &manifest_digest, "host.component.wasm");
            if dest.exists() {
                tracing::info!("using cached host: {}", dest.display());
                dest
            } else {
                tracing::info!(
                    "defaults host.component.wasm missing; pulling from registry: {}@{} (resolved from {})",
                    desired_host_repo,
                    manifest_digest,
                    desired_host_ref
                );
                crate::registry_pull::pull_component_wasm_to_file(
                    client,
                    &desired_host_repo,
                    &manifest_digest,
                    &dest,
                    false,
                )
                .context("failed to pull host.component.wasm from registry")?;
                tracing::info!("cached host: {}", dest.display());
                dest
            }
        }
    } else {
        let Some(client) = client.as_ref() else {
            bail!("--update-lock requires MOLT_REGISTRY to be configured");
        };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to initialize async runtime")?;
        let (manifest_digest, layer_digest) = rt
            .block_on(client.resolve_component_digests(&desired_host_repo, &desired_host_ref))
            .with_context(|| {
                format!(
                    "failed to resolve host digest from registry: {}:{}",
                    desired_host_repo, desired_host_ref
                )
            })?;

        lock.set_framework_host(crate::lock::LockedComponent {
            repo: desired_host_repo.clone(),
            reference: desired_host_ref.clone(),
            digest: manifest_digest.clone(),
            layer_digest: Some(layer_digest),
        });
        *lock_dirty = true;

        let dest = cache_path(&desired_host_repo, &manifest_digest, "host.component.wasm");
        if dest.exists() {
            tracing::info!("using cached host: {}", dest.display());
            dest
        } else {
            tracing::info!(
                "defaults host.component.wasm missing; pulling from registry: {}@{} (resolved from {})",
                desired_host_repo,
                manifest_digest,
                desired_host_ref
            );
            crate::registry_pull::pull_component_wasm_to_file(
                client,
                &desired_host_repo,
                &manifest_digest,
                &dest,
                false,
            )
            .context("failed to pull host.component.wasm from registry")?;
            tracing::info!("cached host: {}", dest.display());
            dest
        }
    };

    let core_path = if core_local.exists() {
        core_local
    } else if !update_lock {
        if let Some(locked) = lock.framework_core() {
            let digest = locked.digest.trim();
            if digest.is_empty() {
                bail!("wacli.lock has an empty digest for framework core");
            }

            let dest = cache_path(&locked.repo, digest, "core.component.wasm");
            if dest.exists() {
                tracing::info!("using cached core (locked): {}", dest.display());
                dest
            } else {
                let Some(client) = client.as_ref() else {
                    bail!(
                        "framework core is not available locally, and MOLT_REGISTRY is not configured.\n\n\
Expected cached core at: {}\n\n\
Set MOLT_REGISTRY (and auth) to download framework components.",
                        dest.display()
                    );
                };
                tracing::info!(
                    "defaults core.component.wasm missing; pulling locked digest from registry: {}@{}",
                    locked.repo,
                    digest
                );
                crate::registry_pull::pull_component_wasm_to_file(
                    client,
                    &locked.repo,
                    digest,
                    &dest,
                    false,
                )
                .context("failed to pull core.component.wasm from registry (locked)")?;
                tracing::info!("cached core: {}", dest.display());
                dest
            }
        } else {
            let Some(client) = client.as_ref() else {
                return crate::component_scan::verify_defaults(defaults_dir);
            };

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to initialize async runtime")?;
            let (manifest_digest, layer_digest) = rt
                .block_on(client.resolve_component_digests(&desired_core_repo, &desired_core_ref))
                .with_context(|| {
                    format!(
                        "failed to resolve core digest from registry: {}:{}",
                        desired_core_repo, desired_core_ref
                    )
                })?;

            lock.set_framework_core(crate::lock::LockedComponent {
                repo: desired_core_repo.clone(),
                reference: desired_core_ref.clone(),
                digest: manifest_digest.clone(),
                layer_digest: Some(layer_digest),
            });
            *lock_dirty = true;

            let dest = cache_path(&desired_core_repo, &manifest_digest, "core.component.wasm");
            if dest.exists() {
                tracing::info!("using cached core: {}", dest.display());
                dest
            } else {
                tracing::info!(
                    "defaults core.component.wasm missing; pulling from registry: {}@{} (resolved from {})",
                    desired_core_repo,
                    manifest_digest,
                    desired_core_ref
                );
                crate::registry_pull::pull_component_wasm_to_file(
                    client,
                    &desired_core_repo,
                    &manifest_digest,
                    &dest,
                    false,
                )
                .context("failed to pull core.component.wasm from registry")?;
                tracing::info!("cached core: {}", dest.display());
                dest
            }
        }
    } else {
        let Some(client) = client.as_ref() else {
            bail!("--update-lock requires MOLT_REGISTRY to be configured");
        };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to initialize async runtime")?;
        let (manifest_digest, layer_digest) = rt
            .block_on(client.resolve_component_digests(&desired_core_repo, &desired_core_ref))
            .with_context(|| {
                format!(
                    "failed to resolve core digest from registry: {}:{}",
                    desired_core_repo, desired_core_ref
                )
            })?;

        lock.set_framework_core(crate::lock::LockedComponent {
            repo: desired_core_repo.clone(),
            reference: desired_core_ref.clone(),
            digest: manifest_digest.clone(),
            layer_digest: Some(layer_digest),
        });
        *lock_dirty = true;

        let dest = cache_path(&desired_core_repo, &manifest_digest, "core.component.wasm");
        if dest.exists() {
            tracing::info!("using cached core: {}", dest.display());
            dest
        } else {
            tracing::info!(
                "defaults core.component.wasm missing; pulling from registry: {}@{} (resolved from {})",
                desired_core_repo,
                manifest_digest,
                desired_core_ref
            );
            crate::registry_pull::pull_component_wasm_to_file(
                client,
                &desired_core_repo,
                &manifest_digest,
                &dest,
                false,
            )
            .context("failed to pull core.component.wasm from registry")?;
            tracing::info!("cached core: {}", dest.display());
            dest
        }
    };

    if !host_path.exists() {
        bail!("failed to resolve host.component.wasm (local missing, cache missing)");
    }
    if !core_path.exists() {
        bail!("failed to resolve core.component.wasm (local missing, cache missing)");
    }

    Ok((host_path, core_path))
}

fn resolve_registry_commands(
    base_dir: &Path,
    commands: &[manifest::RegistryCommand],
    update_lock: bool,
    lock: &mut crate::lock::LockFile,
    lock_dirty: &mut bool,
) -> Result<Vec<crate::component_scan::CommandInfo>> {
    if commands.is_empty() {
        return Ok(Vec::new());
    }

    let client = molt_registry_client::OciWasmClient::from_env()?;
    if update_lock && client.is_none() {
        bail!("--update-lock requires MOLT_REGISTRY to be configured");
    }

    let cache_dir = base_dir.join(".wacli").join("commands");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create directory: {}", cache_dir.display()))?;

    let refresh = std::env::var("WACLI_REGISTRY_REFRESH")
        .map(|v| !v.trim().is_empty() && v.trim() != "0")
        .unwrap_or(false);

    // Runtime for digest resolution when MOLT_REGISTRY is configured.
    let rt = if client.is_some() {
        Some(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to initialize async runtime")?,
        )
    } else {
        None
    };

    let referenced_names: Vec<String> =
        commands.iter().map(|c| c.name.trim().to_string()).collect();

    let mut out = Vec::with_capacity(commands.len());
    for cmd in commands {
        if cmd.repo.trim().is_empty() {
            bail!("build.commands entry for '{}' has an empty repo", cmd.name);
        }
        if cmd.reference.trim().is_empty() {
            bail!(
                "build.commands entry for '{}' has an empty reference",
                cmd.name
            );
        }
        if !crate::component_scan::is_valid_command_name(cmd.name.trim()) {
            bail!(
                "invalid command name '{}' in build.commands (must match [a-z][a-z0-9-]*)",
                cmd.name
            );
        }

        let repo = cmd.repo.trim().to_string();
        let reference = cmd.reference.trim().to_string();
        let name = cmd.name.trim().to_string();

        let locked = lock.find_command(&name).cloned();

        let mut resolved_via_registry = false;
        let (manifest_digest, layer_digest) = if update_lock {
            let Some(client) = client.as_ref() else {
                bail!("--update-lock requires MOLT_REGISTRY to be configured");
            };
            let rt = rt.as_ref().expect("runtime must exist when client exists");
            resolved_via_registry = true;
            let (digest, layer) = rt
                .block_on(client.resolve_component_digests(&repo, &reference))
                .with_context(|| {
                    format!("failed to resolve digest for command {name} from {repo}:{reference}")
                })?;
            (digest, Some(layer))
        } else if let Some(locked) = locked.as_ref() {
            if locked.repo != repo || locked.reference != reference {
                bail!(
                    "wacli.lock is out of date for command '{}':\n  lock: {}:{}\n  manifest: {}:{}\n\nRun: wacli build --update-lock",
                    name,
                    locked.repo,
                    locked.reference,
                    repo,
                    reference
                );
            }
            let digest = locked.digest.trim();
            if digest.is_empty() {
                bail!("wacli.lock has an empty digest for command '{name}'");
            }
            (digest.to_string(), locked.layer_digest.clone())
        } else {
            let Some(client) = client.as_ref() else {
                bail!(
                    "command '{}' is not locked and MOLT_REGISTRY is not configured.\n\n\
Set MOLT_REGISTRY (and auth) or run once with registry configured to generate wacli.lock.",
                    name
                );
            };
            let rt = rt.as_ref().expect("runtime must exist when client exists");
            resolved_via_registry = true;
            let (digest, layer) = rt
                .block_on(client.resolve_component_digests(&repo, &reference))
                .with_context(|| {
                    format!("failed to resolve digest for command {name} from {repo}:{reference}")
                })?;
            (digest, Some(layer))
        };

        // If we resolved a digest (update-lock or first pull), update the lock entry.
        let should_write_lock = resolved_via_registry || locked.is_none();
        if should_write_lock {
            lock.set_command(crate::lock::LockedRegistryCommand {
                name: name.clone(),
                repo: repo.clone(),
                reference: reference.clone(),
                digest: manifest_digest.clone(),
                layer_digest,
            });
            *lock_dirty = true;
        }

        let dest = cache_dir
            .join(molt_registry_client::sanitize_path_segment(&repo))
            .join(molt_registry_client::sanitize_path_segment(
                &manifest_digest,
            ))
            .join(format!("{}.component.wasm", name));

        if dest.exists() && !refresh {
            tracing::info!(
                "using cached command {} from {}@{}",
                name,
                repo,
                manifest_digest
            );
        } else {
            let Some(client) = client.as_ref() else {
                bail!(
                    "command '{}' is not cached and MOLT_REGISTRY is not configured.\n\n\
Expected cached component at: {}\n\n\
Set MOLT_REGISTRY (and auth) to download.",
                    name,
                    dest.display()
                );
            };
            tracing::info!(
                "pulling command {} from registry {}@{} (resolved from {})",
                name,
                repo,
                manifest_digest,
                reference
            );
            crate::registry_pull::pull_component_wasm_to_file(
                client,
                &repo,
                &manifest_digest,
                &dest,
                refresh,
            )
            .with_context(|| {
                format!("failed to pull command {name} from {repo}@{manifest_digest}")
            })?;
        }

        let info = crate::component_scan::inspect_command_component(&dest)
            .with_context(|| format!("invalid command component for '{name}'"))?;
        if info.name != name {
            bail!(
                "registry command name mismatch: expected '{}', got '{}' (file: {})",
                name,
                info.name,
                dest.display()
            );
        }
        out.push(info);
    }

    if update_lock {
        lock.commands
            .retain(|c| referenced_names.iter().any(|n| n == &c.name));
    }

    Ok(out)
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

fn compose(args: ComposeArgs) -> Result<()> {
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

#[cfg(feature = "runtime")]
fn run(args: RunArgs) -> Result<()> {
    let runner = plugin_loader::Runner::new()?;
    let mut preopens = Vec::new();
    for dir in &args.dirs {
        preopens.push(parse_preopen_dir(dir)?);
    }
    let (extra_dirs, passthrough_args) = split_run_args(&args.args)?;
    for dir in extra_dirs {
        preopens.push(parse_preopen_dir(&dir)?);
    }
    let code = runner.run_component_with_preopens(&args.component, &passthrough_args, &preopens)?;
    if code != 0 {
        std::process::exit(code as i32);
    }
    Ok(())
}

#[cfg(feature = "runtime")]
fn parse_preopen_dir(value: &str) -> Result<plugin_loader::PreopenDir> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("--dir value is empty");
    }
    let (host, guest) = match trimmed.split_once("::") {
        Some((host, guest)) => (host.trim(), guest.trim()),
        None => (trimmed, trimmed),
    };
    if host.is_empty() {
        bail!("--dir host path is empty");
    }
    if guest.is_empty() {
        bail!("--dir guest path is empty");
    }
    Ok(plugin_loader::PreopenDir::new(host, guest))
}

#[cfg(feature = "runtime")]
fn split_run_args(args: &[String]) -> Result<(Vec<String>, Vec<String>)> {
    let mut preopens = Vec::new();
    let mut passthrough = Vec::new();
    let mut i = 0usize;
    let mut stop = false;

    while i < args.len() {
        let arg = &args[i];
        if !stop {
            if arg == "--" {
                stop = true;
                passthrough.push(arg.clone());
                i += 1;
                continue;
            }
            if arg == "--dir" {
                let next = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("--dir requires a value (HOST[::GUEST])"))?;
                preopens.push(next.clone());
                i += 2;
                continue;
            }
            if let Some(rest) = arg.strip_prefix("--dir=") {
                if rest.trim().is_empty() {
                    bail!("--dir requires a value (HOST[::GUEST])");
                }
                preopens.push(rest.to_string());
                i += 1;
                continue;
            }
        }

        passthrough.push(arg.clone());
        i += 1;
    }

    Ok((preopens, passthrough))
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

fn self_update(args: SelfUpdateArgs) -> Result<()> {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "wacli-linux-x86_64",
        ("linux", "aarch64") => "wacli-linux-aarch64",
        ("macos", "x86_64") => "wacli-macos-x86_64",
        ("macos", "aarch64") => "wacli-macos-aarch64",
        ("windows", "x86_64") => "wacli-windows-x86_64.exe",
        _ => bail!(
            "unsupported platform for self-update: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    };

    let mut updater = Update::configure();
    if let Some(version) = &args.version {
        let tag = format!("v{version}");
        updater.target_version_tag(&tag);
    }

    let updater = updater
        .repo_owner("RAKUDEJI")
        .repo_name("wacli")
        .bin_name("wacli")
        .target(target)
        .identifier(".zip")
        .show_download_progress(true)
        .no_confirm(true)
        .current_version(env!("CARGO_PKG_VERSION"))
        .build()
        .context("failed to configure updater")?;

    let status = updater.update().context("failed to update")?;

    match status {
        Status::UpToDate(version) => {
            eprintln!("wacli is already up to date ({}).", version);
        }
        Status::Updated(version) => {
            eprintln!("wacli updated to {}.", version);
        }
    }

    Ok(())
}
