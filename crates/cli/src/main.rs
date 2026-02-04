mod component_scan;
mod registry_gen_wat;
mod wac_gen;
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

use crate::component_scan::{scan_commands, verify_defaults};
use crate::registry_gen_wat::{
    generate_registry_wat, get_prebuilt_registry, should_use_prebuilt_registry,
};
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

    #[cfg(feature = "runtime")]
    /// Run a composed CLI component with dynamic pipes
    Run(RunArgs),

    /// Update wacli from GitHub Releases
    SelfUpdate(SelfUpdateArgs),
}

#[derive(Parser)]
struct BuildArgs {
    /// Package name (e.g., "example:my-cli")
    #[arg(long, default_value = "example:my-cli")]
    name: String,

    /// Package version
    #[arg(long, default_value = "0.1.0")]
    version: String,

    /// Output file path
    #[arg(short, long, default_value = "my-cli.component.wasm")]
    output: PathBuf,

    /// Skip validation of the composed component
    #[arg(long)]
    no_validate: bool,

    /// Print generated WAC without composing
    #[arg(long)]
    print_wac: bool,
}

#[derive(Parser)]
struct InitArgs {
    /// Project directory (default: current directory)
    #[arg(value_name = "DIR")]
    dir: Option<PathBuf>,

    /// Download framework components (host/core) into defaults/
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

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init(args) => init(args),
        Commands::Build(args) => build(args),
        Commands::Compose(args) => compose(args),
        Commands::Plug(args) => plug(args),
        #[cfg(feature = "runtime")]
        Commands::Run(args) => run(args),
        Commands::SelfUpdate(args) => self_update(args),
    }
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
        download_framework_components(&defaults_dir, args.overwrite)?;
    }

    eprintln!("Created:");
    eprintln!("  {}", defaults_dir.display());
    eprintln!("  {}", commands_dir.display());
    eprintln!("  {}", dir.join("wit").display());
    eprintln!();
    eprintln!("Next steps:");
    if args.with_components {
        eprintln!("  1. Place your command components in commands/");
        eprintln!("  2. Run: wacli build");
    } else {
        eprintln!("  1. Place host.component.wasm and core.component.wasm in defaults/");
        eprintln!("  2. Place your command components in commands/");
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
const HOST_COMPONENT_URL: &str =
    "https://github.com/RAKUDEJI/wacli/releases/latest/download/host.component.wasm";
const CORE_COMPONENT_URL: &str =
    "https://github.com/RAKUDEJI/wacli/releases/latest/download/core.component.wasm";

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
        fs::remove_file(&dest)
            .with_context(|| format!("failed to remove {}", dest.display()))?;
    }
    fs::rename(&tmp_path, &dest)
        .with_context(|| format!("failed to move {} into place", dest.display()))?;
    tracing::info!("installed {} -> {}", name, dest.display());
    Ok(())
}

fn download_framework_components(defaults_dir: &Path, overwrite: bool) -> Result<()> {
    let host_path = defaults_dir.join("host.component.wasm");
    let core_path = defaults_dir.join("core.component.wasm");

    download_component(
        HOST_COMPONENT_URL,
        &host_path,
        overwrite,
        "host.component.wasm",
    )?;
    download_component(
        CORE_COMPONENT_URL,
        &core_path,
        overwrite,
        "core.component.wasm",
    )?;
    Ok(())
}

fn download_component(url: &str, dest: &Path, overwrite: bool, label: &str) -> Result<()> {
    if dest.exists() && !overwrite {
        tracing::info!("{} already exists, skipping download", label);
        return Ok(());
    }

    let tmp_path = dest.with_extension("download");
    let response = ureq::get(url)
        .set("User-Agent", concat!("wacli/", env!("CARGO_PKG_VERSION")))
        .call()
        .with_context(|| format!("failed to download {}", url))?;

    if response.status() >= 400 {
        bail!(
            "failed to download {} (status {})",
            label,
            response.status()
        );
    }

    let mut reader = response.into_reader();
    let mut tmp_file = fs::File::create(&tmp_path)
        .with_context(|| format!("failed to create {}", tmp_path.display()))?;
    std::io::copy(&mut reader, &mut tmp_file)
        .with_context(|| format!("failed to write {}", tmp_path.display()))?;

    if overwrite && dest.exists() {
        fs::remove_file(dest)
            .with_context(|| format!("failed to remove {}", dest.display()))?;
    }

    fs::rename(&tmp_path, dest)
        .with_context(|| format!("failed to move {} into place", label))?;
    tracing::info!("downloaded {} -> {}", label, dest.display());
    Ok(())
}

fn build(args: BuildArgs) -> Result<()> {
    tracing::debug!("executing build command");

    let defaults_dir = PathBuf::from("defaults");
    let commands_dir = PathBuf::from("commands");

    // Verify required defaults exist
    let (host_path, core_path) = verify_defaults(&defaults_dir)?;

    // Scan and validate commands
    let commands = scan_commands(&commands_dir)?;

    tracing::info!("found {} command(s)", commands.len());
    for cmd in &commands {
        tracing::debug!("  - {}: {}", cmd.name, cmd.path.display());
    }

    // Get registry (pre-built or generate)
    // Use the minimal registry.wit that doesn't have WASI dependencies
    let registry_path = if should_use_prebuilt_registry(&defaults_dir) {
        get_prebuilt_registry(&defaults_dir).unwrap()
    } else {
        // Generate registry component
        tracing::info!("generating registry component...");
        tracing::info!("using WAT template registry generator");
        let registry_bytes =
            generate_registry_wat(&commands).context("failed to generate registry (WAT)")?;

        // Write to defaults directory
        let generated_path = defaults_dir.join("registry.component.wasm");
        fs::write(&generated_path, &registry_bytes)
            .context("failed to write generated registry")?;
        tracing::info!("generated: {}", generated_path.display());

        generated_path
    };

    // Generate WAC
    let wac_source = generate_wac(&args.name, &commands);

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

    // Create output directory if needed
    if let Some(parent) = args.output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Write output
    fs::write(&args.output, &bytes)
        .with_context(|| format!("failed to write output file: {}", args.output.display()))?;

    eprintln!("Built: {}", args.output.display());

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
    let code = runner.run_component(&args.component, &args.args)?;
    if code != 0 {
        std::process::exit(code as i32);
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
