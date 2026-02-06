#![allow(clippy::all)]

mod bindings;

use bindings::export;
use bindings::exports::wacli::cli::{host_env, host_fs, host_io, host_pipes, host_process};
use bindings::wacli::cli::pipe_runtime;
use bindings::wasi;
use wasi::filesystem::types::{Descriptor, DescriptorFlags, ErrorCode, OpenFlags, PathFlags};

struct HostProvider;
struct HostPipe {
    inner: pipe_runtime::Pipe,
}

impl host_env::Guest for HostProvider {
    fn args() -> Vec<String> {
        wasi::cli::environment::get_arguments()
    }

    fn env() -> Vec<(String, String)> {
        wasi::cli::environment::get_environment()
    }
}

impl host_io::Guest for HostProvider {
    fn stdout_write(bytes: Vec<u8>) {
        write_output(bytes, StreamTarget::Stdout);
    }

    fn stderr_write(bytes: Vec<u8>) {
        write_output(bytes, StreamTarget::Stderr);
    }

    fn stdout_flush() {
        flush_output(StreamTarget::Stdout);
    }

    fn stderr_flush() {
        flush_output(StreamTarget::Stderr);
    }
}

impl host_fs::Guest for HostProvider {
    fn read_file(path: String) -> Result<Vec<u8>, String> {
        if path.is_empty() {
            return Err("path is empty".to_string());
        }
        let (dir, rel_path) = resolve_preopen_path(&path)?;
        let file = dir
            .open_at(
                PathFlags::SYMLINK_FOLLOW,
                &rel_path,
                OpenFlags::empty(),
                DescriptorFlags::READ,
            )
            .map_err(|e| fs_error("read", &path, e))?;
        let mut out = Vec::new();
        let mut offset = 0u64;
        loop {
            let (chunk, eof) = file
                .read(64 * 1024, offset)
                .map_err(|e| fs_error("read", &path, e))?;
            if chunk.is_empty() {
                break;
            }
            offset += chunk.len() as u64;
            out.extend_from_slice(&chunk);
            if eof {
                break;
            }
        }
        Ok(out)
    }

    fn write_file(path: String, contents: Vec<u8>) -> Result<(), String> {
        if path.is_empty() {
            return Err("path is empty".to_string());
        }
        let (dir, rel_path) = resolve_preopen_path(&path)?;
        let file = dir
            .open_at(
                PathFlags::SYMLINK_FOLLOW,
                &rel_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE,
                DescriptorFlags::WRITE,
            )
            .map_err(|e| fs_error("write", &path, e))?;
        let mut offset = 0u64;
        while offset < contents.len() as u64 {
            let written = file
                .write(&contents[offset as usize..], offset)
                .map_err(|e| fs_error("write", &path, e))?;
            if written == 0 {
                return Err("write returned 0 bytes".to_string());
            }
            offset += written;
        }
        Ok(())
    }

    fn create_dir(path: String) -> Result<(), String> {
        if path.is_empty() {
            return Err("path is empty".to_string());
        }
        let (dir, rel_path) = resolve_preopen_path(&path)?;
        if rel_path == "." {
            return Err(format!(
                "cannot create the preopened directory itself: {path}"
            ));
        }
        dir.create_directory_at(&rel_path)
            .map_err(|e| fs_error("create-dir", &path, e))?;
        Ok(())
    }

    fn list_dir(path: String) -> Result<Vec<String>, String> {
        let requested = if path.is_empty() { "." } else { path.as_str() };
        let (dir, rel_path) = resolve_preopen_path(requested)?;
        let target = if rel_path == "." {
            dir
        } else {
            dir.open_at(
                PathFlags::SYMLINK_FOLLOW,
                &rel_path,
                OpenFlags::DIRECTORY,
                DescriptorFlags::READ,
            )
            .map_err(|e| fs_error("list-dir", requested, e))?
        };
        let stream = target
            .read_directory()
            .map_err(|e| fs_error("list-dir", requested, e))?;
        let mut out = Vec::new();
        loop {
            let entry = stream
                .read_directory_entry()
                .map_err(|e| fs_error("list-dir", requested, e))?;
            match entry {
                Some(entry) => out.push(entry.name),
                None => break,
            }
        }
        Ok(out)
    }
}

impl host_process::Guest for HostProvider {
    fn exit(code: u32) {
        if code == 0 {
            wasi::cli::exit::exit(Ok(()));
        } else {
            wasi::cli::exit::exit(Err(()));
        }
    }
}

impl host_pipes::Guest for HostProvider {
    type Pipe = HostPipe;

    fn list_pipes() -> Vec<host_pipes::PipeInfo> {
        pipe_runtime::list_pipes()
            .into_iter()
            .map(convert_pipe_info)
            .collect()
    }

    fn load_pipe(name: String) -> Result<host_pipes::Pipe, String> {
        pipe_runtime::load_pipe(&name).map(|pipe| host_pipes::Pipe::new(HostPipe { inner: pipe }))
    }
}

impl host_pipes::GuestPipe for HostPipe {
    fn meta(&self) -> host_pipes::PipeMeta {
        convert_pipe_meta(self.inner.meta())
    }

    fn process(
        &self,
        input: Vec<u8>,
        options: Vec<String>,
    ) -> Result<Vec<u8>, host_pipes::PipeError> {
        self.inner
            .process(&input, &options)
            .map_err(convert_pipe_error)
    }
}

export!(HostProvider with_types_in bindings);

enum StreamTarget {
    Stdout,
    Stderr,
}

fn write_output(bytes: Vec<u8>, target: StreamTarget) {
    if bytes.is_empty() {
        return;
    }

    match target {
        StreamTarget::Stdout => {
            let stream = wasi::cli::stdout::get_stdout();
            let _ = stream.blocking_write_and_flush(&bytes);
        }
        StreamTarget::Stderr => {
            let stream = wasi::cli::stderr::get_stderr();
            let _ = stream.blocking_write_and_flush(&bytes);
        }
    }
}

fn flush_output(target: StreamTarget) {
    match target {
        StreamTarget::Stdout => {
            let stream = wasi::cli::stdout::get_stdout();
            let _ = stream.blocking_flush();
        }
        StreamTarget::Stderr => {
            let stream = wasi::cli::stderr::get_stderr();
            let _ = stream.blocking_flush();
        }
    }
}

fn resolve_preopen_path(path: &str) -> Result<(Descriptor, String), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("path is empty".to_string());
    }

    let mut dirs = wasi::filesystem::preopens::get_directories();
    if dirs.is_empty() {
        return Err("no preopened directories available".to_string());
    }

    // Relative paths always resolve against "." (current directory) if present.
    if !path.starts_with('/') {
        let idx = dirs.iter().position(|(_, name)| name == ".").unwrap_or(0);
        let (dir, _) = dirs.swap_remove(idx);
        return Ok((dir, path.to_string()));
    }

    // Absolute guest paths are resolved against a matching preopen name such as "/data".
    // We choose the longest matching mount point.
    let mut best: Option<(usize, usize, String)> = None;
    for (idx, (_, name)) in dirs.iter().enumerate() {
        let mount = normalize_mount(name);
        if mount == "." {
            continue;
        }
        if let Some(rel) = strip_mount(path, mount) {
            let score = mount.len();
            match &best {
                Some((_, best_score, _)) if *best_score >= score => {}
                _ => best = Some((idx, score, rel)),
            }
        }
    }

    if let Some((idx, _score, rel)) = best {
        let (dir, _) = dirs.swap_remove(idx);
        return Ok((dir, rel));
    }

    let available: Vec<&str> = dirs.iter().map(|(_, name)| name.as_str()).collect();
    Err(format!(
        "path is outside preopened directories: {path} (available: {})",
        available.join(", ")
    ))
}

fn normalize_mount(name: &str) -> &str {
    let trimmed = name.trim();
    if trimmed == "/" || trimmed == "." {
        return trimmed;
    }
    trimmed.trim_end_matches('/')
}

fn strip_mount(path: &str, mount: &str) -> Option<String> {
    if mount == "." {
        return None;
    }

    // Root mount.
    if mount == "/" {
        let rest = path.strip_prefix('/')?;
        let rest = rest.trim_start_matches('/');
        if rest.is_empty() {
            return Some(".".to_string());
        }
        return Some(rest.to_string());
    }

    if path == mount {
        return Some(".".to_string());
    }

    if path.starts_with(mount) {
        let bytes = path.as_bytes();
        if bytes.get(mount.len()) == Some(&b'/') {
            let rest = path[mount.len() + 1..].trim_start_matches('/');
            if rest.is_empty() {
                return Some(".".to_string());
            }
            return Some(rest.to_string());
        }
    }

    None
}

fn fs_error(op: &str, path: &str, err: ErrorCode) -> String {
    match err {
        ErrorCode::Access | ErrorCode::NotPermitted => {
            format!("{op}: permission denied: {path} ({})", err.name())
        }
        ErrorCode::NoEntry => format!("{op}: not found: {path} ({})", err.name()),
        ErrorCode::NotDirectory => format!("{op}: not a directory: {path} ({})", err.name()),
        ErrorCode::IsDirectory => format!("{op}: is a directory: {path} ({})", err.name()),
        ErrorCode::ReadOnly => format!("{op}: read-only filesystem: {path} ({})", err.name()),
        _ => format!("{op}: filesystem error: {path} ({})", err.name()),
    }
}

fn convert_pipe_info(info: pipe_runtime::PipeInfo) -> host_pipes::PipeInfo {
    host_pipes::PipeInfo {
        name: info.name,
        summary: info.summary,
        path: info.path,
    }
}

fn convert_pipe_meta(meta: pipe_runtime::PipeMeta) -> host_pipes::PipeMeta {
    host_pipes::PipeMeta {
        name: meta.name,
        summary: meta.summary,
        input_types: meta.input_types,
        output_type: meta.output_type,
        version: meta.version,
    }
}

fn convert_pipe_error(err: pipe_runtime::PipeError) -> host_pipes::PipeError {
    match err {
        pipe_runtime::PipeError::ParseError(msg) => host_pipes::PipeError::ParseError(msg),
        pipe_runtime::PipeError::TransformError(msg) => host_pipes::PipeError::TransformError(msg),
        pipe_runtime::PipeError::InvalidOption(msg) => host_pipes::PipeError::InvalidOption(msg),
    }
}
