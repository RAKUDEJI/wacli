//! wacli command development kit for Rust plugins.
//!
//! # Example
//!
//! ```rust,ignore
//! use wacli_cdk::{Command, CommandMeta, CommandResult, meta};
//!
//! struct Hello;
//!
//! impl Command for Hello {
//!     fn meta() -> CommandMeta {
//!         meta("hello").summary("Say hello").build()
//!     }
//!
//!     fn run(argv: Vec<String>) -> CommandResult {
//!         wacli_cdk::io::println("Hello!");
//!         Ok(0)
//!     }
//! }
//!
//! wacli_cdk::export!(Hello);
//! ```

#[doc(hidden)]
pub mod bindings;

pub use bindings::wacli::cli::{host_env, host_fs, host_io, host_pipes, host_process};
pub use bindings::wacli::cli::types::{
    CommandError, CommandMeta, CommandResult, PipeError, PipeInfo, PipeMeta,
};

impl From<String> for CommandError {
    fn from(s: String) -> Self {
        CommandError::Io(s)
    }
}

impl From<&str> for CommandError {
    fn from(s: &str) -> Self {
        CommandError::Io(s.to_string())
    }
}

impl From<PipeError> for CommandError {
    fn from(e: PipeError) -> Self {
        match e {
            PipeError::ParseError(msg) => CommandError::Failed(msg),
            PipeError::TransformError(msg) => CommandError::Failed(msg),
            PipeError::InvalidOption(msg) => CommandError::InvalidArgs(msg),
        }
    }
}

#[doc(hidden)]
#[allow(dead_code)]
#[used]
static __WACLI_FORCE_HOST_IMPORTS: ForceHostImports = ForceHostImports {
    env_args: host_env::args,
    env_env: host_env::env,
    io_stdout_write: host_io::stdout_write,
    io_stderr_write: host_io::stderr_write,
    io_stdout_flush: host_io::stdout_flush,
    io_stderr_flush: host_io::stderr_flush,
    fs_read: host_fs::read_file,
    fs_write: host_fs::write_file,
    fs_create: host_fs::create_dir,
    fs_list: host_fs::list_dir,
    process_exit: host_process::exit,
    pipes_list: host_pipes::list_pipes,
    pipes_load: host_pipes::load_pipe,
    pipe_meta: host_pipes::Pipe::meta,
    pipe_process: host_pipes::Pipe::process,
};

#[doc(hidden)]
#[allow(dead_code)]
struct ForceHostImports {
    env_args: fn() -> Vec<String>,
    env_env: fn() -> Vec<(String, String)>,
    io_stdout_write: fn(&[u8]),
    io_stderr_write: fn(&[u8]),
    io_stdout_flush: fn(),
    io_stderr_flush: fn(),
    fs_read: fn(&str) -> Result<Vec<u8>, String>,
    fs_write: fn(&str, &[u8]) -> Result<(), String>,
    fs_create: fn(&str) -> Result<(), String>,
    fs_list: fn(&str) -> Result<Vec<String>, String>,
    process_exit: fn(u32),
    pipes_list: fn() -> Vec<PipeInfo>,
    pipes_load: fn(&str) -> Result<host_pipes::Pipe, String>,
    pipe_meta: fn(&host_pipes::Pipe) -> PipeMeta,
    pipe_process: fn(&host_pipes::Pipe, &[u8], &[String]) -> Result<Vec<u8>, PipeError>,
}

/// Convenience facade over the split host interfaces.
pub mod host {
    pub use super::host_env::{args, env};
    pub use super::host_fs::{create_dir, list_dir, read_file, write_file};
    pub use super::host_io::{stderr_flush, stderr_write, stdout_flush, stdout_write};
    pub use super::host_pipes::{list_pipes, load_pipe, Pipe};
    pub use super::host_process::exit;
}

/// Common imports for wacli command implementations.
pub mod prelude {
    pub use super::{
        args, fs, io, meta, pipes, Command, CommandError, CommandMeta, CommandResult, Context,
    };
}

/// Exit code type for commands.
pub type ExitCode = u32;

/// Execution context for commands.
#[derive(Debug, Clone)]
pub struct Context {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Context {
    pub fn new(argv: Vec<String>) -> Self {
        Self {
            argv,
            env: host::env(),
        }
    }

    /// Get the positional argument at the given index.
    pub fn arg(&self, index: usize) -> Option<&str> {
        args::positional(&self.argv, index)
    }

    /// Get all positional arguments (flags and their values are skipped).
    pub fn positional_args(&self) -> Vec<&str> {
        args::positional_args(&self.argv)
    }

    /// Check if a flag like `--help` exists.
    ///
    /// Accepts a single name or multiple names via array/slice.
    pub fn flag<'a, N>(&self, names: N) -> bool
    where
        N: args::FlagNames<'a>,
    {
        args::flag(&self.argv, names)
    }

    /// Get a flag value such as `--name=value` or `--name value`.
    pub fn value(&self, name: &str) -> Option<&str> {
        args::value(&self.argv, name)
    }

    /// Require a positional argument by index.
    pub fn require_arg(&self, index: usize, name: &str) -> Result<&str, CommandError> {
        self.arg(index).ok_or_else(|| {
            CommandError::InvalidArgs(format!("missing required argument: {name}"))
        })
    }
}

/// Trait for implementing a wacli command.
pub trait Command {
    /// Return command metadata.
    fn meta() -> CommandMeta;

    /// Execute the command with the given arguments.
    fn run(argv: Vec<String>) -> CommandResult;
}

/// Export a command implementation.
///
/// This macro generates the WASM exports required by the wacli plugin interface.
///
/// # Example
///
/// ```rust,ignore
/// struct MyCommand;
///
/// impl wacli_cdk::Command for MyCommand {
///     fn meta() -> wacli_cdk::CommandMeta {
///         wacli_cdk::meta("my-cmd").build()
///     }
///
///     fn run(argv: Vec<String>) -> wacli_cdk::CommandResult {
///         Ok(0)
///     }
/// }
///
/// wacli_cdk::export!(MyCommand);
/// ```
#[macro_export]
macro_rules! export {
    ($ty:ty) => {
        const _: () = {
            struct __WacliShim;

            impl $crate::bindings::exports::wacli::cli::command::Guest for __WacliShim {
                fn meta() -> $crate::CommandMeta {
                    <$ty as $crate::Command>::meta()
                }

                fn run(argv: Vec<String>) -> $crate::CommandResult {
                    <$ty as $crate::Command>::run(argv)
                }
            }

            #[unsafe(export_name = "wacli:cli/command@1.0.0#meta")]
            unsafe extern "C" fn __export_meta() -> *mut u8 {
                unsafe {
                    $crate::bindings::exports::wacli::cli::command::_export_meta_cabi::<__WacliShim>()
                }
            }

            #[unsafe(export_name = "wacli:cli/command@1.0.0#run")]
            unsafe extern "C" fn __export_run(arg0: *mut u8, arg1: usize) -> *mut u8 {
                unsafe {
                    $crate::bindings::exports::wacli::cli::command::_export_run_cabi::<__WacliShim>(
                        arg0, arg1,
                    )
                }
            }

            #[unsafe(export_name = "cabi_post_wacli:cli/command@1.0.0#meta")]
            unsafe extern "C" fn __post_return_meta(arg0: *mut u8) {
                unsafe {
                    $crate::bindings::exports::wacli::cli::command::__post_return_meta::<__WacliShim>(
                        arg0,
                    )
                }
            }

            #[unsafe(export_name = "cabi_post_wacli:cli/command@1.0.0#run")]
            unsafe extern "C" fn __post_return_run(arg0: *mut u8) {
                unsafe {
                    $crate::bindings::exports::wacli::cli::command::__post_return_run::<__WacliShim>(
                        arg0,
                    )
                }
            }
        };
    };
}

/// Create a metadata builder.
///
/// # Example
///
/// ```rust,ignore
/// wacli_cdk::meta("greet")
///     .summary("Greet someone")
///     .usage("greet [NAME]")
///     .version("1.0.0")
///     .build()
/// ```
pub fn meta(name: impl Into<String>) -> MetaBuilder {
    MetaBuilder::new(name)
}

/// Minimal argument helpers (no extra dependencies).
pub mod args {
    /// Argument name collection for flag matching.
    pub trait FlagNames<'a> {
        type Iter: Iterator<Item = &'a str>;
        fn iter(self) -> Self::Iter;
    }

    impl<'a> FlagNames<'a> for &'a str {
        type Iter = std::iter::Once<&'a str>;

        fn iter(self) -> Self::Iter {
            std::iter::once(self)
        }
    }

    impl<'a> FlagNames<'a> for &'a [&'a str] {
        type Iter = std::iter::Copied<std::slice::Iter<'a, &'a str>>;

        fn iter(self) -> Self::Iter {
            self.iter().copied()
        }
    }

    impl<'a, const N: usize> FlagNames<'a> for [&'a str; N] {
        type Iter = std::array::IntoIter<&'a str, N>;

        fn iter(self) -> Self::Iter {
            self.into_iter()
        }
    }

    /// Check if a flag like `--help` exists.
    ///
    /// Accepts a single name or multiple names via array/slice.
    /// Parsing stops at `--`.
    pub fn flag<'a, N>(argv: &[String], names: N) -> bool
    where
        N: FlagNames<'a>,
    {
        let names: Vec<&str> = names.iter().collect();
        for arg in argv {
            if arg == "--" {
                break;
            }
            if names.iter().any(|name| arg == name) {
                return true;
            }
        }
        false
    }

    /// Get a flag value like `--name=value` or `--name value`.
    ///
    /// Parsing stops at `--`.
    pub fn value<'a>(argv: &'a [String], name: &str) -> Option<&'a str> {
        let needle = format!("{name}=");
        for (idx, arg) in argv.iter().enumerate() {
            if arg == "--" {
                break;
            }
            if let Some(rest) = arg.strip_prefix(&needle) {
                return Some(rest);
            }
            if arg == name {
                return argv.get(idx + 1).map(|s| s.as_str());
            }
        }
        None
    }

    /// Get all positional arguments.
    ///
    /// Flags (arguments starting with `-`) are skipped. If a flag looks like it
    /// takes a value (`--key value` or `-k value`), the value is also skipped.
    /// Use `--` to stop flag parsing and treat everything after as positional.
    pub fn positional_args<'a>(argv: &'a [String]) -> Vec<&'a str> {
        let mut positionals = Vec::new();
        let mut i = 0;
        let mut after_separator = false;

        while i < argv.len() {
            let arg = &argv[i];
            if !after_separator {
                if arg == "--" {
                    after_separator = true;
                    i += 1;
                    continue;
                }
                if arg != "-" && arg.starts_with('-') {
                    if arg.contains('=') {
                        i += 1;
                        continue;
                    }
                    let next = argv.get(i + 1);
                    if arg.starts_with("--") {
                        if let Some(next) = next {
                            if !next.starts_with('-') {
                                i += 2;
                                continue;
                            }
                        }
                        i += 1;
                        continue;
                    }
                    if arg.len() == 2 {
                        if let Some(next) = next {
                            if !next.starts_with('-') {
                                i += 2;
                                continue;
                            }
                        }
                    }
                    i += 1;
                    continue;
                }
            }

            positionals.push(arg.as_str());
            i += 1;
        }

        positionals
    }

    /// Get a positional argument by index.
    pub fn positional<'a>(argv: &'a [String], index: usize) -> Option<&'a str> {
        positional_args(argv).get(index).copied()
    }

    /// Get the remaining arguments from a start index.
    pub fn rest<'a>(argv: &'a [String], start: usize) -> &'a [String] {
        if start >= argv.len() {
            &argv[argv.len()..]
        } else {
            &argv[start..]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::args;

    #[test]
    fn positional_skips_flags() {
        let argv = vec!["--loud".to_string(), "--".to_string(), "Bob".to_string()];
        assert_eq!(args::positional(&argv, 0), Some("Bob"));
    }

    #[test]
    fn positional_skips_flag_values() {
        let argv = vec![
            "--format".to_string(),
            "json".to_string(),
            "file.txt".to_string(),
        ];
        assert_eq!(args::positional(&argv, 0), Some("file.txt"));
    }

    #[test]
    fn positional_skips_short_flag_values() {
        let argv = vec!["-o".to_string(), "out.txt".to_string(), "file".to_string()];
        assert_eq!(args::positional(&argv, 0), Some("file"));
    }

    #[test]
    fn positional_after_separator() {
        let argv = vec![
            "--".to_string(),
            "--not-a-flag".to_string(),
            "Bob".to_string(),
        ];
        assert_eq!(args::positional(&argv, 0), Some("--not-a-flag"));
        assert_eq!(args::positional(&argv, 1), Some("Bob"));
    }

    #[test]
    fn flag_multiple_names() {
        let argv = vec!["-l".to_string(), "Bob".to_string()];
        assert!(args::flag(&argv, ["-l", "--loud"]));
    }

    #[test]
    fn flag_stops_at_separator() {
        let argv = vec!["--".to_string(), "--loud".to_string()];
        assert!(!args::flag(&argv, "--loud"));
    }

    #[test]
    fn value_stops_at_separator() {
        let argv = vec!["--".to_string(), "--name".to_string(), "Bob".to_string()];
        assert_eq!(args::value(&argv, "--name"), None);
    }
}

/// Builder for `CommandMeta`.
#[derive(Default)]
pub struct MetaBuilder {
    name: String,
    summary: String,
    usage: String,
    aliases: Vec<String>,
    version: String,
    hidden: bool,
    description: String,
    examples: Vec<String>,
}

impl MetaBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }

    pub fn usage(mut self, usage: impl Into<String>) -> Self {
        self.usage = usage.into();
        self
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn example(mut self, example: impl Into<String>) -> Self {
        self.examples.push(example.into());
        self
    }

    pub fn build(self) -> CommandMeta {
        CommandMeta {
            name: self.name,
            summary: self.summary,
            usage: self.usage,
            aliases: self.aliases,
            version: self.version,
            hidden: self.hidden,
            description: self.description,
            examples: self.examples,
        }
    }
}

/// I/O helpers for stdout/stderr.
pub mod io {
    use super::host;

    /// Write to stdout.
    pub fn print(s: impl AsRef<str>) {
        host::stdout_write(s.as_ref().as_bytes());
    }

    /// Write to stderr.
    pub fn eprint(s: impl AsRef<str>) {
        host::stderr_write(s.as_ref().as_bytes());
    }

    /// Write to stdout with newline.
    pub fn println(s: impl AsRef<str>) {
        let mut buf = s.as_ref().as_bytes().to_vec();
        buf.push(b'\n');
        host::stdout_write(&buf);
    }

    /// Write to stderr with newline.
    pub fn eprintln(s: impl AsRef<str>) {
        let mut buf = s.as_ref().as_bytes().to_vec();
        buf.push(b'\n');
        host::stderr_write(&buf);
    }

    /// Flush stdout.
    pub fn flush() {
        host::stdout_flush();
    }
}

/// File system helpers via the host interface.
pub mod fs {
    use super::host;

    /// Read an entire file into memory.
    pub fn read(path: impl AsRef<str>) -> Result<Vec<u8>, String> {
        host::read_file(path.as_ref())
    }

    /// Write a file, creating or truncating it.
    pub fn write(path: impl AsRef<str>, contents: impl AsRef<[u8]>) -> Result<(), String> {
        host::write_file(path.as_ref(), contents.as_ref())
    }

    /// Create a directory.
    pub fn create_dir(path: impl AsRef<str>) -> Result<(), String> {
        host::create_dir(path.as_ref())
    }

    /// List entries in a directory.
    pub fn list_dir(path: impl AsRef<str>) -> Result<Vec<String>, String> {
        host::list_dir(path.as_ref())
    }
}

/// Pipe loader helpers via the host-pipes interface.
pub mod pipes {
    use super::host_pipes;
    use super::PipeInfo;

    /// List available pipes.
    pub fn list() -> Vec<PipeInfo> {
        host_pipes::list_pipes()
    }

    /// Load a pipe by name.
    pub fn load(name: impl AsRef<str>) -> Result<host_pipes::Pipe, String> {
        host_pipes::load_pipe(name.as_ref())
    }
}
