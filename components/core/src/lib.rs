#![allow(clippy::all)]

mod bindings;

use bindings::export;
use bindings::exports::wasi::cli::run;
use bindings::wacli::cli::{host_env, host_io, host_process, registry, types};

struct Core;

impl run::Guest for Core {
    fn run() -> Result<(), ()> {
        let args = host_env::args();
        let argv = trim_program_name(args);

        if argv.is_empty() {
            print_help();
            return Ok(());
        }

        if is_help_flag(&argv[0]) {
            print_help();
            return Ok(());
        }

        let cmd_name = argv[0].clone();
        let cmd_args = argv.get(1..).unwrap_or(&[]).to_vec();

        match registry::run(&cmd_name, &cmd_args) {
            Ok(code) => {
                if code != 0 {
                    host_process::exit(code);
                }
                Ok(())
            }
            Err(err) => {
                report_command_error(&cmd_name, err);
                host_process::exit(1);
                Ok(())
            }
        }
    }
}

export!(Core with_types_in bindings);

fn trim_program_name(mut args: Vec<String>) -> Vec<String> {
    if !args.is_empty() {
        args.remove(0);
    }
    args
}

fn is_help_flag(arg: &str) -> bool {
    arg == "-h" || arg == "--help" || arg == "help"
}

fn print_help() {
    let mut out = String::new();
    out.push_str("Available commands:\n");

    let mut cmds = registry::list_commands();
    cmds.sort_by(|a, b| a.name.cmp(&b.name));

    for cmd in cmds {
        if cmd.name.is_empty() {
            continue;
        }
        if cmd.summary.is_empty() {
            out.push_str(&format!("  {}\n", cmd.name));
        } else {
            out.push_str(&format!("  {:<16} {}\n", cmd.name, cmd.summary));
        }
    }

    host_io::stdout_write(out.as_bytes());
    host_io::stdout_flush();
}

fn report_command_error(name: &str, err: types::CommandError) {
    let message = match err {
        types::CommandError::UnknownCommand(cmd) => {
            format!("Unknown command: {cmd}\n")
        }
        types::CommandError::InvalidArgs(msg)
        | types::CommandError::Failed(msg)
        | types::CommandError::Io(msg) => {
            format!("{msg}\n")
        }
    };

    let mut out = String::new();
    out.push_str(&message);
    out.push_str("Run with --help to see available commands.\n");

    host_io::stderr_write(out.as_bytes());
    host_io::stderr_flush();

    if name == "" {
        print_help();
    }
}
