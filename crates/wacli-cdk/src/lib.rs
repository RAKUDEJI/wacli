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

pub use bindings::wacli::cli::host;
pub use bindings::wacli::cli::types::{CommandError, CommandMeta, CommandResult};

/// Exit code type for commands.
pub type ExitCode = u32;

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
