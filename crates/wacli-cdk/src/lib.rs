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
#[allow(unused_imports)]
pub mod bindings;

pub use bindings::wacli::cli::types::{
    ArgDef, CommandError, CommandMeta, CommandResult, PipeError, PipeInfo, PipeMeta,
};
pub use bindings::wacli::cli::{host_env, host_fs, host_io, host_pipes, host_process};

// Proc-macro helpers (compile-time only).
pub use wacli_cdk_macros::declare_command_metadata;

// Trait impls for shared argparse helpers.
impl wacli_argparse::claplike::ArgDefLike for ArgDef {
    fn name(&self) -> &str {
        &self.name
    }

    fn short(&self) -> Option<&str> {
        self.short.as_deref()
    }

    fn long(&self) -> Option<&str> {
        self.long.as_deref()
    }

    fn help(&self) -> &str {
        &self.help
    }

    fn required(&self) -> bool {
        self.required
    }

    fn default_value(&self) -> Option<&str> {
        self.default_value.as_deref()
    }

    fn value_name(&self) -> Option<&str> {
        self.value_name.as_deref()
    }

    fn takes_value(&self) -> bool {
        self.takes_value
    }
}

impl wacli_argparse::claplike::CommandMetaLike for CommandMeta {
    type ArgDef = ArgDef;

    fn name(&self) -> &str {
        &self.name
    }

    fn summary(&self) -> &str {
        &self.summary
    }

    fn usage(&self) -> &str {
        &self.usage
    }

    fn aliases(&self) -> &[String] {
        self.aliases.as_slice()
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn hidden(&self) -> bool {
        self.hidden
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn examples(&self) -> &[String] {
        self.examples.as_slice()
    }

    fn args(&self) -> &[Self::ArgDef] {
        self.args.as_slice()
    }
}

impl From<String> for CommandError {
    fn from(s: String) -> Self {
        // A plain String is an unclassified error message.
        // Treat it as a generic failure instead of an I/O error.
        CommandError::Failed(s)
    }
}

impl From<&str> for CommandError {
    fn from(s: &str) -> Self {
        CommandError::Failed(s.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Io(e.to_string())
    }
}

impl From<std::string::FromUtf8Error> for CommandError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        CommandError::Failed(e.to_string())
    }
}

impl From<std::str::Utf8Error> for CommandError {
    fn from(e: std::str::Utf8Error) -> Self {
        CommandError::Failed(e.to_string())
    }
}

impl From<std::num::ParseIntError> for CommandError {
    fn from(e: std::num::ParseIntError) -> Self {
        CommandError::InvalidArgs(e.to_string())
    }
}

impl From<std::num::ParseFloatError> for CommandError {
    fn from(e: std::num::ParseFloatError) -> Self {
        CommandError::InvalidArgs(e.to_string())
    }
}

impl From<std::str::ParseBoolError> for CommandError {
    fn from(e: std::str::ParseBoolError) -> Self {
        CommandError::InvalidArgs(e.to_string())
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

type PipeProcessFn = fn(&host_pipes::Pipe, &[u8], &[String]) -> Result<Vec<u8>, PipeError>;

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
    pipe_process: PipeProcessFn,
}

/// Convenience facade over the split host interfaces.
pub mod host {
    pub use super::host_env::{args, env};
    pub use super::host_fs::{create_dir, list_dir, read_file, write_file};
    pub use super::host_io::{stderr_flush, stderr_write, stdout_flush, stdout_write};
    pub use super::host_pipes::{Pipe, list_pipes, load_pipe};
    pub use super::host_process::exit;
}

/// Common imports for wacli command implementations.
pub mod prelude {
    pub use super::{
        Command, CommandError, CommandMeta, CommandResult, Context, arg, args, fs, io, meta, pipes,
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

    /// Get all positional arguments.
    ///
    /// This does not guess which flags take a value, so values for `--key value`
    /// are not skipped unless you use `positional_args_with_schema`.
    pub fn positional_args(&self) -> Vec<&str> {
        args::positional_args(&self.argv)
    }

    /// Get the positional argument at the given index using a schema.
    pub fn arg_with_schema(&self, index: usize, schema: &args::Schema) -> Option<&str> {
        args::positional_with_schema(&self.argv, index, schema)
    }

    /// Get all positional arguments using a schema to skip values of declared flags.
    pub fn positional_args_with_schema(&self, schema: &args::Schema) -> Vec<&str> {
        args::positional_args_with_schema(&self.argv, schema)
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
        self.arg(index)
            .ok_or_else(|| CommandError::InvalidArgs(format!("missing required argument: {name}")))
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

            #[unsafe(export_name = "wacli:cli/command@2.0.0#meta")]
            unsafe extern "C" fn __export_meta() -> *mut u8 {
                unsafe {
                    $crate::bindings::exports::wacli::cli::command::_export_meta_cabi::<__WacliShim>()
                }
            }

            #[unsafe(export_name = "wacli:cli/command@2.0.0#run")]
            unsafe extern "C" fn __export_run(arg0: *mut u8, arg1: usize) -> *mut u8 {
                unsafe {
                    $crate::bindings::exports::wacli::cli::command::_export_run_cabi::<__WacliShim>(
                        arg0, arg1,
                    )
                }
            }

            #[unsafe(export_name = "cabi_post_wacli:cli/command@2.0.0#meta")]
            unsafe extern "C" fn __post_return_meta(arg0: *mut u8) {
                unsafe {
                    $crate::bindings::exports::wacli::cli::command::__post_return_meta::<__WacliShim>(
                        arg0,
                    )
                }
            }

            #[unsafe(export_name = "cabi_post_wacli:cli/command@2.0.0#run")]
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

/// Create an argument definition builder.
///
/// This is used to declaratively describe accepted flags and positional arguments
/// in `CommandMeta`.
///
/// # Example
///
/// ```rust,ignore
/// use wacli_cdk::{arg, meta};
///
/// fn meta() -> wacli_cdk::CommandMeta {
///     meta("show")
///         .arg(arg("file").required(true).value_name("FILE").help("File to display"))
///         .arg(arg("verbose").short("-v").long("--verbose").help("Verbose output"))
///         .build()
/// }
/// ```
pub fn arg(name: impl Into<String>) -> ArgBuilder {
    ArgBuilder::new(name)
}

/// Parse `argv` according to the declarative argument definitions in `meta`.
pub fn parse<'a>(
    meta: &CommandMeta,
    argv: &'a [String],
) -> Result<args::Matches<'a>, CommandError> {
    args::parse(meta, argv)
}

/// Minimal argument helpers (no extra dependencies).
pub mod args {
    pub use wacli_argparse::args::{
        FlagNames, Matches, Schema, flag, positional, positional_args, positional_args_with_schema,
        positional_with_schema, rest, value,
    };

    use super::{CommandError, CommandMeta};
    use wacli_argparse::claplike::{self, ParseOutcome};

    /// Render a help message based on `CommandMeta`.
    pub fn help(meta: &CommandMeta) -> String {
        claplike::help(meta)
    }

    /// Render a version message based on `CommandMeta`.
    pub fn version(meta: &CommandMeta) -> String {
        claplike::version(meta)
    }

    /// Parse `argv` based on the `meta.args` schema.
    ///
    /// This implements a minimal clap-like behavior:
    /// - `-h/--help` prints auto-generated help and exits 0
    /// - `-V/--version` prints version and exits 0
    /// - required argument checks
    /// - unknown flag detection
    pub fn parse<'a>(meta: &CommandMeta, argv: &'a [String]) -> Result<Matches<'a>, CommandError> {
        match claplike::parse(meta, argv) {
            Ok(ParseOutcome::Matches(m)) => Ok(m),
            Ok(ParseOutcome::Help(msg)) | Ok(ParseOutcome::Version(msg)) => {
                #[cfg(target_arch = "wasm32")]
                {
                    super::io::print(&msg);
                    super::io::flush();
                    super::host::exit(0);
                    return Err(CommandError::Failed(
                        "unexpected return from host_process::exit".into(),
                    ));
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    Err(CommandError::InvalidArgs(msg))
                }
            }
            Err(claplike::ParseError::InvalidArgs(msg)) => Err(CommandError::InvalidArgs(msg)),
            Err(claplike::ParseError::Failed(msg)) => Err(CommandError::Failed(msg)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{arg, args, meta, parse};

    #[test]
    fn positional_skips_flags() {
        let argv = vec!["--loud".to_string(), "--".to_string(), "Bob".to_string()];
        assert_eq!(args::positional(&argv, 0), Some("Bob"));
    }

    #[test]
    fn positional_does_not_guess_long_flag_values() {
        let argv = vec![
            "--format".to_string(),
            "json".to_string(),
            "file.txt".to_string(),
        ];
        assert_eq!(args::positional(&argv, 0), Some("json"));
    }

    #[test]
    fn positional_does_not_guess_short_flag_values() {
        let argv = vec!["-o".to_string(), "out.txt".to_string(), "file".to_string()];
        assert_eq!(args::positional(&argv, 0), Some("out.txt"));
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
    fn positional_keeps_positional_after_boolean_flags() {
        let argv = vec!["--verbose".to_string(), "hello.txt".to_string()];
        assert_eq!(args::positional(&argv, 0), Some("hello.txt"));

        let argv = vec!["-v".to_string(), "hello.txt".to_string()];
        assert_eq!(args::positional(&argv, 0), Some("hello.txt"));
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

    #[test]
    fn positional_with_schema_skips_known_value_flags() {
        let argv = vec![
            "--format".to_string(),
            "json".to_string(),
            "file.txt".to_string(),
        ];
        let schema = args::Schema::new().value_flag("--format");
        assert_eq!(
            args::positional_with_schema(&argv, 0, &schema),
            Some("file.txt")
        );
        assert_eq!(
            args::positional_args_with_schema(&argv, &schema),
            vec!["file.txt"]
        );
    }

    #[test]
    fn parse_does_not_consume_positional_for_boolean_flag() {
        let meta = meta("show")
            .arg(arg("verbose").long("--verbose").help("Verbose output"))
            .arg(
                arg("file")
                    .required(true)
                    .value_name("FILE")
                    .help("File to show"),
            )
            .build();
        let argv = vec!["--verbose".to_string(), "hello.txt".to_string()];
        let m = parse(&meta, &argv).unwrap();
        assert!(m.is_present("verbose"));
        assert_eq!(m.get("file"), Some("hello.txt"));
    }

    #[test]
    fn parse_consumes_value_for_value_flag_even_if_it_starts_with_dash() {
        let meta = meta("show")
            .arg(
                arg("output")
                    .long("--output")
                    .value_name("FILE")
                    .help("Output file"),
            )
            .arg(
                arg("file")
                    .required(true)
                    .value_name("FILE")
                    .help("Input file"),
            )
            .build();
        let argv = vec![
            "--output".to_string(),
            "-".to_string(),
            "in.txt".to_string(),
        ];
        let m = parse(&meta, &argv).unwrap();
        assert_eq!(m.get("output"), Some("-"));
        assert_eq!(m.get("file"), Some("in.txt"));
    }

    #[test]
    fn parse_supports_combined_short_flags_and_attached_value() {
        let meta = meta("show")
            .arg(arg("verbose").short("-v").help("Verbose output"))
            .arg(
                arg("output")
                    .short("-o")
                    .value_name("FILE")
                    .help("Output file"),
            )
            .arg(
                arg("file")
                    .required(true)
                    .value_name("FILE")
                    .help("Input file"),
            )
            .build();
        let argv = vec!["-voout.txt".to_string(), "in.txt".to_string()];
        let m = parse(&meta, &argv).unwrap();
        assert!(m.is_present("verbose"));
        assert_eq!(m.get("output"), Some("out.txt"));
        assert_eq!(m.get("file"), Some("in.txt"));
    }

    #[test]
    fn parse_applies_default_value_for_missing_value_flag() {
        let meta = meta("show")
            .arg(
                arg("format")
                    .long("--format")
                    .value_name("PIPE")
                    .default_value("plain")
                    .help("Format pipe"),
            )
            .build();
        let argv: Vec<String> = Vec::new();
        let m = parse(&meta, &argv).unwrap();
        assert_eq!(m.get("format"), Some("plain"));
    }

    #[test]
    fn parse_errors_on_missing_required_positional() {
        let meta = meta("show")
            .arg(
                arg("file")
                    .required(true)
                    .value_name("FILE")
                    .help("File to show"),
            )
            .build();
        let argv: Vec<String> = Vec::new();
        let err = parse(&meta, &argv).unwrap_err();
        match err {
            super::CommandError::InvalidArgs(msg) => {
                assert_eq!(msg, "missing required argument: <FILE>");
            }
            other => panic!("expected InvalidArgs, got: {other:?}"),
        }
    }

    #[test]
    fn parse_errors_on_missing_required_option() {
        let meta = meta("show")
            .arg(
                arg("output")
                    .long("--output")
                    .required(true)
                    .value_name("FILE")
                    .help("Output file"),
            )
            .build();
        let argv: Vec<String> = Vec::new();
        let err = parse(&meta, &argv).unwrap_err();
        match err {
            super::CommandError::InvalidArgs(msg) => {
                assert_eq!(msg, "missing required argument: --output <FILE>");
            }
            other => panic!("expected InvalidArgs, got: {other:?}"),
        }
    }

    #[test]
    fn help_renders_options_and_args_sections() {
        let meta = meta("show")
            .summary("Show a file")
            .usage("show [OPTIONS] <FILE>")
            .description("Display a file to stdout.")
            .example("show hello.txt")
            .arg(
                arg("file")
                    .required(true)
                    .value_name("FILE")
                    .help("File to show"),
            )
            .arg(
                arg("verbose")
                    .short("-v")
                    .long("--verbose")
                    .help("Verbose output"),
            )
            .build();
        let text = args::help(&meta);
        assert!(text.contains("Usage: show [OPTIONS] <FILE>"));
        assert!(text.contains("Arguments:"));
        assert!(text.contains("<FILE>"));
        assert!(text.contains("Options:"));
        assert!(text.contains("--help"));
        assert!(text.contains("--version"));
        assert!(text.contains("--verbose"));
        assert!(text.contains("Examples:"));
        assert!(text.contains("show hello.txt"));
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
    args: Vec<ArgDef>,
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

    pub fn arg(mut self, arg: ArgBuilder) -> Self {
        self.args.push(arg.build());
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
            args: self.args,
        }
    }
}

/// Builder for `ArgDef`.
#[derive(Default)]
pub struct ArgBuilder {
    name: String,
    short: Option<String>,
    long: Option<String>,
    help: String,
    required: bool,
    default_value: Option<String>,
    value_name: Option<String>,
    takes_value: Option<bool>,
}

impl ArgBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn short(mut self, short: impl Into<String>) -> Self {
        self.short = Some(short.into());
        self
    }

    pub fn long(mut self, long: impl Into<String>) -> Self {
        self.long = Some(long.into());
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default_value = Some(value.into());
        self
    }

    pub fn value_name(mut self, value_name: impl Into<String>) -> Self {
        self.value_name = Some(value_name.into());
        self
    }

    pub fn takes_value(mut self, takes_value: bool) -> Self {
        self.takes_value = Some(takes_value);
        self
    }

    pub fn build(self) -> ArgDef {
        let short = self.short.map(|s| {
            let s = s.trim().to_string();
            if s.starts_with('-') {
                s
            } else {
                format!("-{s}")
            }
        });
        let long = self.long.map(|s| {
            let s = s.trim().to_string();
            if s.starts_with('-') {
                s
            } else {
                format!("--{s}")
            }
        });

        let positional = short.is_none() && long.is_none();
        let inferred_takes_value = if positional {
            true
        } else {
            self.value_name.is_some() || self.default_value.is_some()
        };
        ArgDef {
            name: self.name,
            short,
            long,
            help: self.help,
            required: self.required,
            default_value: self.default_value,
            value_name: self.value_name,
            takes_value: self.takes_value.unwrap_or(inferred_takes_value),
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
    use super::{CommandError, host};

    /// Read an entire file into memory.
    pub fn read(path: impl AsRef<str>) -> Result<Vec<u8>, CommandError> {
        host::read_file(path.as_ref()).map_err(CommandError::Io)
    }

    /// Write a file, creating or truncating it.
    pub fn write(path: impl AsRef<str>, contents: impl AsRef<[u8]>) -> Result<(), CommandError> {
        host::write_file(path.as_ref(), contents.as_ref()).map_err(CommandError::Io)
    }

    /// Create a directory.
    pub fn create_dir(path: impl AsRef<str>) -> Result<(), CommandError> {
        host::create_dir(path.as_ref()).map_err(CommandError::Io)
    }

    /// List entries in a directory.
    pub fn list_dir(path: impl AsRef<str>) -> Result<Vec<String>, CommandError> {
        host::list_dir(path.as_ref()).map_err(CommandError::Io)
    }
}

/// Pipe loader helpers via the host-pipes interface.
pub mod pipes {
    use super::PipeInfo;
    use super::host_pipes;

    /// List available pipes.
    pub fn list() -> Vec<PipeInfo> {
        host_pipes::list_pipes()
    }

    /// Load a pipe by name.
    pub fn load(name: impl AsRef<str>) -> Result<host_pipes::Pipe, String> {
        host_pipes::load_pipe(name.as_ref())
    }
}
