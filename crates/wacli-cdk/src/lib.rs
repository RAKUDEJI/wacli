//! wacli command development kit for Rust plugins.
//!
//! # Example
//! ```rust
//! use wacli_cdk::{export_command, Command, CommandMetaBuilder, CommandResult};
//!
//! struct Hello;
//!
//! impl Command for Hello {
//!     fn meta() -> wacli_cdk::CommandMeta {
//!         CommandMetaBuilder::new("hello")
//!             .summary("Say hello")
//!             .build()
//!     }
//!
//!     fn run(_argv: Vec<String>) -> CommandResult {
//!         wacli_cdk::io::stdout_println("Hello from Rust!");
//!         Ok(0)
//!     }
//! }
//!
//! export_command!(Hello);
//! ```

wit_bindgen::generate!({
    world: "plugin",
    path: "wit/command.wit",
    pub_export_macro: true,
});

use core::marker::PhantomData;

pub use wacli::cli::host;
pub use wacli::cli::types::{CommandError, CommandMeta, CommandResult};

/// Exit code type for commands.
pub type ExitCode = u32;

/// Trait implemented by command plugins.
pub trait Command {
    /// Command metadata.
    fn meta() -> CommandMeta;

    /// Execute the command with argv.
    fn run(argv: Vec<String>) -> CommandResult;
}

/// Adapter to bridge `Command` into the WIT guest trait.
pub struct CommandShim<T>(PhantomData<T>);

impl<T: Command> exports::wacli::cli::command::Guest for CommandShim<T> {
    fn meta() -> CommandMeta {
        T::meta()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        T::run(argv)
    }
}

/// Export a command implementation as a wacli plugin.
#[macro_export]
macro_rules! export_command {
    ($ty:ty) => {
        $crate::export!($crate::CommandShim::<$ty>);
    };
}

/// Builder for `CommandMeta` with sensible defaults.
#[derive(Default)]
pub struct CommandMetaBuilder {
    name: String,
    summary: String,
    usage: String,
    aliases: Vec<String>,
    version: String,
    hidden: bool,
    description: String,
    examples: Vec<String>,
}

impl CommandMetaBuilder {
    /// Create a new builder with the required name.
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

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
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

/// Convenience helpers for writing to stdout/stderr.
pub mod io {
    use super::host;

    pub fn stdout_write_str(s: impl AsRef<str>) {
        host::stdout_write(s.as_ref().as_bytes());
    }

    pub fn stderr_write_str(s: impl AsRef<str>) {
        host::stderr_write(s.as_ref().as_bytes());
    }

    pub fn stdout_println(s: impl AsRef<str>) {
        let mut buf = s.as_ref().as_bytes().to_vec();
        buf.push(b'\n');
        host::stdout_write(&buf);
    }

    pub fn stderr_println(s: impl AsRef<str>) {
        let mut buf = s.as_ref().as_bytes().to_vec();
        buf.push(b'\n');
        host::stderr_write(&buf);
    }
}
