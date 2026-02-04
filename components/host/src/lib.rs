#![allow(clippy::all)]

mod bindings;

use bindings::export;
use bindings::exports::wacli::cli::host;
use bindings::wasi;
use wasi::filesystem::types::{Descriptor, DescriptorFlags, ErrorCode, OpenFlags, PathFlags};

struct HostProvider;

impl host::Guest for HostProvider {
    fn args() -> Vec<String> {
        wasi::cli::environment::get_arguments()
    }

    fn env() -> Vec<(String, String)> {
        wasi::cli::environment::get_environment()
    }

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

    fn exit(code: u32) {
        if code == 0 {
            wasi::cli::exit::exit(Ok(()));
        } else {
            wasi::cli::exit::exit(Err(()));
        }
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
