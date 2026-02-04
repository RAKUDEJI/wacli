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
        let dir = pick_preopen()?;
        let file = dir
            .open_at(PathFlags::SYMLINK_FOLLOW, &path, OpenFlags::empty(), DescriptorFlags::READ)
            .map_err(fs_error)?;
        let mut out = Vec::new();
        let mut offset = 0u64;
        loop {
            let (chunk, eof) = file
                .read(64 * 1024, offset)
                .map_err(fs_error)?;
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
        let dir = pick_preopen()?;
        let file = dir
            .open_at(
                PathFlags::SYMLINK_FOLLOW,
                &path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE,
                DescriptorFlags::WRITE,
            )
            .map_err(fs_error)?;
        let mut offset = 0u64;
        while offset < contents.len() as u64 {
            let written = file
                .write(&contents[offset as usize..], offset)
                .map_err(fs_error)?;
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
        let dir = pick_preopen()?;
        dir.create_directory_at(&path).map_err(fs_error)?;
        Ok(())
    }

    fn list_dir(path: String) -> Result<Vec<String>, String> {
        let dir = pick_preopen()?;
        let target = if path.is_empty() || path == "." {
            dir
        } else {
            dir.open_at(
                PathFlags::SYMLINK_FOLLOW,
                &path,
                OpenFlags::DIRECTORY,
                DescriptorFlags::READ,
            )
            .map_err(fs_error)?
        };
        let stream = target.read_directory().map_err(fs_error)?;
        let mut out = Vec::new();
        loop {
            let entry = stream.read_directory_entry().map_err(fs_error)?;
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
        pipe_runtime::load_pipe(&name)
            .map(|pipe| host_pipes::Pipe::new(HostPipe { inner: pipe }))
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

fn pick_preopen() -> Result<Descriptor, String> {
    let mut dirs = wasi::filesystem::preopens::get_directories();
    if dirs.is_empty() {
        return Err("no preopened directories available".to_string());
    }
    let idx = dirs
        .iter()
        .position(|(_, name)| name == ".")
        .unwrap_or(0);
    let (dir, _) = dirs.swap_remove(idx);
    Ok(dir)
}

fn fs_error(err: ErrorCode) -> String {
    format!("filesystem error: {err:?}")
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
        pipe_runtime::PipeError::TransformError(msg) => {
            host_pipes::PipeError::TransformError(msg)
        }
        pipe_runtime::PipeError::InvalidOption(msg) => {
            host_pipes::PipeError::InvalidOption(msg)
        }
    }
}
