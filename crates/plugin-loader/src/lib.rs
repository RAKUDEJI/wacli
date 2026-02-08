use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::p2;
use wasmtime_wasi::p2::bindings::sync::Command;
use wasmtime_wasi::{DirPerms, FilePerms, I32Exit, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

mod pipe_plugin_bindings {
    #![allow(clippy::all, dead_code, unused_imports, unused_mut, unused_variables)]
    include!("bindings/pipe_plugin.rs");
}

#[derive(Default)]
struct PipeState;

pub struct LoadedPipe {
    store: Store<PipeState>,
    instance: pipe_plugin_bindings::PipePlugin,
    meta: pipe_plugin_bindings::wacli::cli::types::PipeMeta,
}

#[derive(Debug, Clone)]
pub struct PreopenDir {
    pub host: PathBuf,
    pub guest: String,
}

impl PreopenDir {
    pub fn new(host: impl Into<PathBuf>, guest: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            guest: guest.into(),
        }
    }
}

mod pipe_runtime_bindings {
    #![allow(clippy::all, dead_code, unused_imports, unused_mut, unused_variables)]
    include!("bindings/pipe_runtime_host.rs");
}

use pipe_runtime_bindings::wacli::cli::{pipe_runtime, types as pipe_types};

#[cfg(feature = "regen-bindings")]
mod regen_bindings {
    #![allow(clippy::all, dead_code, unused_imports, unused_mut, unused_variables)]
    mod pipe_plugin {
        #![allow(clippy::all, dead_code, unused_imports, unused_mut, unused_variables)]
        wasmtime::component::bindgen!({
            path: "../../wit/cli",
            world: "pipe-plugin",
        });
    }
    mod pipe_runtime_host {
        #![allow(clippy::all, dead_code, unused_imports, unused_mut, unused_variables)]
        wasmtime::component::bindgen!({
            path: "../../wit/cli",
            world: "pipe-runtime-host",
            with: {
                "wacli:cli/pipe-runtime.pipe": crate::LoadedPipe,
            },
        });
    }
}

/// Runs a composed CLI component with dynamic pipe loading.
pub struct Runner {
    engine: Engine,
}

impl Runner {
    /// Create a runner with component model enabled.
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        let engine = Engine::new(&config).context("failed to create wasmtime engine")?;
        Ok(Self { engine })
    }

    /// Run a composed CLI component (.component.wasm).
    pub fn run_component(&self, component_path: impl AsRef<Path>, args: &[String]) -> Result<u32> {
        self.run_component_with_preopens(component_path, args, &[])
    }

    /// Run a composed CLI component with extra preopened directories.
    pub fn run_component_with_preopens(
        &self,
        component_path: impl AsRef<Path>,
        args: &[String],
        preopens: &[PreopenDir],
    ) -> Result<u32> {
        let component_path = component_path.as_ref();
        let component = Component::from_file(&self.engine, component_path)
            .with_context(|| format!("failed to load component: {}", component_path.display()))?;

        let mut linker = Linker::new(&self.engine);
        p2::add_to_linker_sync(&mut linker).context("failed to add WASI to linker")?;
        pipe_runtime_bindings::PipeRuntimeHost::add_to_linker::<
            HostState,
            wasmtime::component::HasSelf<HostState>,
        >(&mut linker, |state: &mut HostState| state)
        .context("failed to add pipe-runtime to linker")?;

        let program_name = component_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("wacli")
            .to_string();
        let mut wasi_args = Vec::with_capacity(args.len() + 1);
        wasi_args.push(program_name);
        wasi_args.extend_from_slice(args);

        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdio().inherit_env().args(&wasi_args);
        builder
            .preopened_dir(".", ".", DirPerms::all(), FilePerms::all())
            .context("failed to preopen current directory")?;
        for dir in preopens {
            if dir.guest.trim().is_empty() {
                return Err(anyhow::anyhow!("guest path for --dir cannot be empty"));
            }
            let host = dir.host.as_path();
            if !host.exists() {
                return Err(anyhow::anyhow!(
                    "preopen directory not found: {}",
                    host.display()
                ));
            }
            if !host.is_dir() {
                return Err(anyhow::anyhow!(
                    "preopen path is not a directory: {}",
                    host.display()
                ));
            }
            builder
                .preopened_dir(host, &dir.guest, DirPerms::all(), FilePerms::all())
                .with_context(|| {
                    format!(
                        "failed to preopen directory {} as {}",
                        host.display(),
                        dir.guest
                    )
                })?;
        }
        let ctx = builder.build();

        let current_command = detect_command(args);
        let plugins_dir = PathBuf::from("plugins");

        let mut store = Store::new(
            &self.engine,
            HostState {
                ctx,
                table: ResourceTable::new(),
                engine: self.engine.clone(),
                plugins_dir,
                current_command,
            },
        );

        let command = Command::instantiate(&mut store, &component, &linker)
            .context("failed to instantiate component")?;
        match command.wasi_cli_run().call_run(&mut store) {
            Ok(Ok(())) => Ok(0),
            Ok(Err(())) => Ok(1),
            Err(err) => {
                if let Some(exit) = err.downcast_ref::<I32Exit>() {
                    Ok(exit.0 as u32)
                } else {
                    Err(err).context("failed to invoke wasi:cli/run")
                }
            }
        }
    }
}

struct HostState {
    ctx: WasiCtx,
    table: ResourceTable,
    engine: Engine,
    plugins_dir: PathBuf,
    current_command: Option<String>,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl pipe_types::Host for HostState {}

impl pipe_runtime::Host for HostState {
    fn list_pipes(&mut self) -> Vec<pipe_runtime::PipeInfo> {
        let base = self.pipes_root();
        if !base.exists() {
            return Vec::new();
        }
        let mut pipes = Vec::new();
        if collect_pipe_infos(&base, &base, &mut pipes).is_err() {
            return Vec::new();
        }
        pipes.sort_by(|a, b| a.name.cmp(&b.name));
        pipes
    }

    fn load_pipe(
        &mut self,
        name: String,
    ) -> Result<wasmtime::component::Resource<LoadedPipe>, String> {
        let normalized = self.resolve_pipe_name(&name)?;
        let path = self.resolve_pipe_path(&normalized)?;
        let pipe = self.instantiate_pipe(&path)?;
        self.table
            .push(pipe)
            .map_err(|e| format!("failed to register pipe: {e}"))
    }
}

impl pipe_runtime::HostPipe for HostState {
    fn meta(&mut self, pipe: wasmtime::component::Resource<LoadedPipe>) -> pipe_runtime::PipeMeta {
        match self.table.get(&pipe) {
            Ok(pipe) => convert_pipe_meta(&pipe.meta),
            Err(err) => pipe_runtime::PipeMeta {
                name: "invalid".to_string(),
                summary: format!("pipe handle is invalid: {err}"),
                input_types: Vec::new(),
                output_type: String::new(),
                version: "0.0.0".to_string(),
            },
        }
    }

    fn process(
        &mut self,
        pipe: wasmtime::component::Resource<LoadedPipe>,
        input: Vec<u8>,
        options: Vec<String>,
    ) -> Result<Vec<u8>, pipe_runtime::PipeError> {
        let pipe = self.table.get_mut(&pipe).map_err(|e| {
            pipe_runtime::PipeError::TransformError(format!("pipe handle is invalid: {e}"))
        })?;
        match pipe
            .instance
            .wacli_cli_pipe()
            .call_process(&mut pipe.store, &input, &options)
        {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(err)) => Err(convert_pipe_error(err)),
            Err(err) => Err(pipe_runtime::PipeError::TransformError(format!(
                "pipe execution failed: {err}"
            ))),
        }
    }

    fn drop(&mut self, pipe: wasmtime::component::Resource<LoadedPipe>) -> wasmtime::Result<()> {
        self.table
            .delete(pipe)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!(e))
    }
}

impl HostState {
    fn pipes_root(&self) -> PathBuf {
        match &self.current_command {
            Some(cmd) => self.plugins_dir.join(cmd),
            None => self.plugins_dir.clone(),
        }
    }

    fn resolve_pipe_name(&self, name: &str) -> Result<String, String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("pipe name is empty".to_string());
        }
        if trimmed.contains('\\') {
            return Err("pipe name must use '/' separators".to_string());
        }
        let mut normalized = trimmed.trim_start_matches('/').to_string();
        if let Some(stripped) = normalized.strip_suffix(".component.wasm") {
            normalized = stripped.to_string();
        }
        if let Some(cmd) = &self.current_command {
            let prefix = format!("{cmd}/");
            if !normalized.starts_with(&prefix) {
                normalized = format!("{prefix}{normalized}");
            }
        }
        if !is_valid_pipe_name(&normalized) {
            return Err(format!("invalid pipe name '{normalized}'"));
        }
        Ok(normalized)
    }

    fn resolve_pipe_path(&self, name: &str) -> Result<PathBuf, String> {
        let base = self.plugins_dir.clone();
        if !base.exists() {
            return Err(format!("pipe directory not found: {}", base.display()));
        }
        let mut path = base.join(name);
        path.set_extension("component.wasm");
        if !path.exists() {
            return Err(format!("pipe not found: {}", path.display()));
        }
        if !path.is_file() {
            return Err(format!("pipe is not a file: {}", path.display()));
        }
        Ok(path)
    }

    fn instantiate_pipe(&self, path: &Path) -> Result<LoadedPipe, String> {
        let bytes =
            fs::read(path).map_err(|e| format!("failed to read pipe {}: {e}", path.display()))?;
        let component = Component::from_binary(&self.engine, &bytes)
            .map_err(|e| format!("failed to parse pipe {}: {e}", path.display()))?;
        let linker = Linker::new(&self.engine);
        let mut store = Store::new(&self.engine, PipeState);
        let instance =
            pipe_plugin_bindings::PipePlugin::instantiate(&mut store, &component, &linker)
                .map_err(|e| format!("failed to instantiate pipe {}: {e}", path.display()))?;
        let meta = instance
            .wacli_cli_pipe()
            .call_meta(&mut store)
            .map_err(|e| format!("failed to read pipe metadata {}: {e}", path.display()))?;
        Ok(LoadedPipe {
            store,
            instance,
            meta,
        })
    }
}

fn detect_command(args: &[String]) -> Option<String> {
    args.iter().find(|arg| !arg.starts_with('-')).cloned()
}

fn collect_pipe_infos(
    base: &Path,
    dir: &Path,
    out: &mut Vec<pipe_runtime::PipeInfo>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_pipe_infos(base, &path, out)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let file_name = match path.file_name() {
            Some(name) => name.to_string_lossy(),
            None => continue,
        };
        if !file_name.ends_with(".component.wasm") {
            continue;
        }
        let rel = match path.strip_prefix(base) {
            Ok(rel) => rel,
            Err(_) => continue,
        };
        let mut rel_str = rel
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        if let Some(stripped) = rel_str.strip_suffix(".component.wasm") {
            rel_str = stripped.to_string();
        } else {
            continue;
        }
        if !is_valid_pipe_name(&rel_str) {
            continue;
        }
        out.push(pipe_runtime::PipeInfo {
            name: rel_str,
            summary: String::new(),
            path: path.display().to_string(),
        });
    }
    Ok(())
}

fn convert_pipe_meta(
    meta: &pipe_plugin_bindings::wacli::cli::types::PipeMeta,
) -> pipe_runtime::PipeMeta {
    pipe_runtime::PipeMeta {
        name: meta.name.clone(),
        summary: meta.summary.clone(),
        input_types: meta.input_types.clone(),
        output_type: meta.output_type.clone(),
        version: meta.version.clone(),
    }
}

fn convert_pipe_error(
    err: pipe_plugin_bindings::wacli::cli::types::PipeError,
) -> pipe_runtime::PipeError {
    match err {
        pipe_plugin_bindings::wacli::cli::types::PipeError::ParseError(msg) => {
            pipe_runtime::PipeError::ParseError(msg)
        }
        pipe_plugin_bindings::wacli::cli::types::PipeError::TransformError(msg) => {
            pipe_runtime::PipeError::TransformError(msg)
        }
        pipe_plugin_bindings::wacli::cli::types::PipeError::InvalidOption(msg) => {
            pipe_runtime::PipeError::InvalidOption(msg)
        }
    }
}

fn is_valid_pipe_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    for segment in name.split('/') {
        if !is_valid_command_name(segment) {
            return false;
        }
    }
    true
}

fn is_valid_command_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            return false;
        }
    }
    !name.ends_with('-')
}
