#![allow(clippy::all)]

mod bindings;

use bindings::export;
use bindings::exports::wasi::cli::run;
use bindings::wacli::cli::{host_env, host_io, host_process, registry, registry_schema, schema, types};

use wacli_argparse::{args as argv, claplike};

struct Core;

impl run::Guest for Core {
    fn run() -> Result<(), ()> {
        let (program, argv) = split_program_and_argv(host_env::args());

        // App-level metadata is provided by the registry as pure data (no plugin execution).
        let app = registry_schema::get_app_meta();

        // Fast path: top-level version does not require loading command schemas.
        if argv.first().is_some_and(|a| a == "-V" || a == "--version") {
            print_global_version(&program, &app);
            return Ok(());
        }

        // Metadata is the single source of truth for:
        // - global command list
        // - command-level help/version
        // - command-level validation
        let schemas = registry_schema::list_schemas();

        if let Err(err) = claplike::validate_aliases(&schemas) {
            print_internal_error(err.message());
            host_process::exit(1);
            return Ok(());
        }

        if argv.is_empty() {
            print_global_help(&app, &schemas);
            return Ok(());
        }

        // Top-level built-ins.
        match argv[0].as_str() {
            "help" => {
                if let Some(topic) = argv.get(1) {
                    if let Some(schema) = find_command_schema(&schemas, topic) {
                        print_command_help(schema);
                    } else {
                        print_unknown_command(topic);
                        print_global_help(&app, &schemas);
                        host_process::exit(1);
                    }
                } else {
                    print_global_help(&app, &schemas);
                }
                return Ok(());
            }
            "-h" | "--help" => {
                print_global_help(&app, &schemas);
                return Ok(());
            }
            _ => {}
        }

        let cmd_name = argv[0].as_str();
        let cmd_args = argv.get(1..).unwrap_or(&[]);

        // Unknown command: let the registry decide the canonical error type/message.
        let Some(schema) = find_command_schema(&schemas, cmd_name) else {
            return dispatch_to_registry(cmd_name, cmd_args);
        };

        // Command-level built-ins should work even if the plugin doesn't call `parse()`.
        if argv::flag(cmd_args, ["-h", "--help"]) {
            print_command_help(schema);
            return Ok(());
        }
        if argv::flag(cmd_args, ["-V", "--version"]) {
            print_command_version(schema);
            return Ok(());
        }

        let env = host_env::env();
        match claplike::validate_with_env(schema, cmd_args, &env) {
            Ok(()) => {}
            Err(claplike::ParseError::InvalidArgs(msg)) => {
                print_invalid_args(&msg, schema);
                host_process::exit(1);
                return Ok(());
            }
            Err(claplike::ParseError::Failed(msg)) => {
                print_internal_error(&msg);
                host_process::exit(1);
                return Ok(());
            }
        }

        // Run by canonical name (so aliases work everywhere).
        match registry::run(&schema.name, cmd_args) {
            Ok(code) => {
                if code != 0 {
                    host_process::exit(code);
                }
                Ok(())
            }
            Err(err) => {
                report_command_error(cmd_name, err);
                host_process::exit(1);
                Ok(())
            }
        }
    }
}

export!(Core with_types_in bindings);

fn dispatch_to_registry(cmd_name: &str, cmd_args: &[String]) -> Result<(), ()> {
    match registry::run(cmd_name, cmd_args) {
        Ok(code) => {
            if code != 0 {
                host_process::exit(code);
            }
            Ok(())
        }
        Err(err) => {
            report_command_error(cmd_name, err);
            host_process::exit(1);
            Ok(())
        }
    }
}

fn split_program_and_argv(mut args: Vec<String>) -> (String, Vec<String>) {
    let program = if args.is_empty() {
        String::new()
    } else {
        args.remove(0)
    };
    (program, args)
}

fn program_display_name(program: &str) -> &str {
    program
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(program)
}

fn print_global_version(program: &str, app: &registry_schema::AppMeta) {
    let name = if app.name.trim().is_empty() {
        program_display_name(program)
    } else {
        app.name.trim()
    };
    let mut out = String::new();
    out.push_str(name);
    out.push(' ');
    if app.version.trim().is_empty() {
        out.push_str(env!("CARGO_PKG_VERSION"));
    } else {
        out.push_str(app.version.trim());
    }
    out.push('\n');
    host_io::stdout_write(out.as_bytes());
    host_io::stdout_flush();
}

fn print_internal_error(msg: &str) {
    let mut out = String::new();
    out.push_str("Internal error: ");
    out.push_str(msg.trim_end());
    out.push('\n');
    host_io::stderr_write(out.as_bytes());
    host_io::stderr_flush();
}

fn print_unknown_command(name: &str) {
    let mut out = String::new();
    out.push_str(&format!("Unknown command: {name}\n"));
    host_io::stderr_write(out.as_bytes());
    host_io::stderr_flush();
}

fn print_global_help(app: &registry_schema::AppMeta, schemas: &[schema::CommandSchema]) {
    let mut out = String::new();
    if !app.name.trim().is_empty() {
        out.push_str(app.name.trim());
        if !app.version.trim().is_empty() {
            out.push(' ');
            out.push_str(app.version.trim());
        }
        out.push('\n');
        if !app.description.trim().is_empty() {
            out.push_str(app.description.trim_end());
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str("Available commands:\n");

    let mut cmds: Vec<&schema::CommandSchema> = schemas.iter().collect();
    cmds.sort_by(|a, b| a.name.cmp(&b.name));

    for cmd in cmds {
        if cmd.name.is_empty() || cmd.hidden {
            continue;
        }
        if cmd.summary.is_empty() {
            out.push_str(&format!("  {}\n", cmd.name));
        } else {
            out.push_str(&format!("  {:<16} {}\n", cmd.name, cmd.summary));
        }
    }

    out.push_str("\nRun `help <command>` or `<command> --help` for more information.\n");

    host_io::stdout_write(out.as_bytes());
    host_io::stdout_flush();
}

fn find_command_schema<'a>(
    schemas: &'a [schema::CommandSchema],
    raw: &str,
) -> Option<&'a schema::CommandSchema> {
    schemas
        .iter()
        .find(|m| m.name == raw)
        .or_else(|| schemas.iter().find(|m| m.aliases.iter().any(|a| a == raw)))
}

fn print_command_help(schema: &schema::CommandSchema) {
    let text = claplike::help(schema);
    host_io::stdout_write(text.as_bytes());
    host_io::stdout_flush();
}

fn print_command_version(schema: &schema::CommandSchema) {
    let text = claplike::version(schema);
    host_io::stdout_write(text.as_bytes());
    host_io::stdout_flush();
}

fn print_invalid_args(msg: &str, schema: &schema::CommandSchema) {
    let mut out = String::new();
    out.push_str(msg.trim_end());
    out.push('\n');
    out.push('\n');
    out.push_str(&claplike::help(schema));

    host_io::stderr_write(out.as_bytes());
    host_io::stderr_flush();
}

fn report_command_error(name: &str, err: types::CommandError) {
    let message = match err {
        types::CommandError::UnknownCommand(cmd) => format!("Unknown command: {cmd}\n"),
        types::CommandError::InvalidArgs(msg)
        | types::CommandError::Failed(msg)
        | types::CommandError::Io(msg) => format!("{msg}\n"),
    };

    let mut out = String::new();
    out.push_str(&message);
    out.push_str("Run with --help to see available commands.\n");

    host_io::stderr_write(out.as_bytes());
    host_io::stderr_flush();

    if name.is_empty() {
        let app = registry_schema::get_app_meta();
        let schemas = registry_schema::list_schemas();
        print_global_help(&app, &schemas);
    }
}

// --- Trait impls for shared argparse helpers ---

impl claplike::ArgDefLike for schema::ArgSchema {
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

    fn env(&self) -> Option<&str> {
        self.env.as_deref()
    }

    fn value_name(&self) -> Option<&str> {
        self.value_name.as_deref()
    }

    fn takes_value(&self) -> bool {
        self.takes_value
    }

    fn multiple(&self) -> bool {
        self.multiple
    }

    fn value_type(&self) -> Option<&str> {
        self.value_type.as_deref()
    }

    fn possible_values(&self) -> &[String] {
        self.possible_values.as_slice()
    }

    fn conflicts_with(&self) -> &[String] {
        self.conflicts_with.as_slice()
    }

    fn requires(&self) -> &[String] {
        self.requires.as_slice()
    }

    fn hidden(&self) -> bool {
        self.hidden
    }
}

impl claplike::CommandMetaLike for schema::CommandSchema {
    type ArgDef = schema::ArgSchema;

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
