#![allow(clippy::all)]

wit_bindgen::generate!({
    path: "wit",
    world: "host-provider",
    generate_all,
});

struct HostProvider;

impl exports::wacli::cli::host::Guest for HostProvider {
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

    fn stdin_read(max_bytes: u32) -> Vec<u8> {
        read_from_stdin(max_bytes)
    }

    fn is_tty_stdout() -> bool {
        wasi::cli::terminal_stdout::get_terminal_stdout().is_some()
    }

    fn is_tty_stderr() -> bool {
        wasi::cli::terminal_stderr::get_terminal_stderr().is_some()
    }

    fn terminal_size() -> Option<(u32, u32)> {
        None
    }

    fn random_bytes(n: u32) -> Vec<u8> {
        wasi::random::random::get_random_bytes(n as u64)
    }

    fn insecure_random_bytes(n: u32) -> Vec<u8> {
        wasi::random::insecure::get_insecure_random_bytes(n as u64)
    }

    fn exit(code: u32) {
        if code == 0 {
            wasi::cli::exit::exit(Ok(()));
        } else {
            wasi::cli::exit::exit(Err(()));
        }
    }
}

export!(HostProvider);

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

fn read_from_stdin(max_bytes: u32) -> Vec<u8> {
    if max_bytes == 0 {
        return Vec::new();
    }

    let stream = wasi::cli::stdin::get_stdin();
    match stream.blocking_read(max_bytes as u64) {
        Ok(mut bytes) => {
            if bytes.len() > max_bytes as usize {
                bytes.truncate(max_bytes as usize);
            }
            bytes
        }
        Err(_) => Vec::new(),
    }
}
