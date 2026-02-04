//! Generate WAC source for component composition.

use crate::component_scan::CommandInfo;

/// Generate WAC source for composing a CLI from discovered commands.
///
/// The generated WAC:
/// 1. Instantiates the host component (WASI bridge)
/// 2. Instantiates each command plugin with host dependencies
/// 3. Instantiates the registry with all commands
/// 4. Instantiates core with host and registry
/// 5. Exports the CLI entry point (wasi:cli/run)
pub fn generate_wac(package_name: &str, commands: &[CommandInfo]) -> String {
    let mut wac = String::new();

    // Package declaration
    wac.push_str(&format!("package {};\n\n", package_name));

    // Instantiate host (imports WASI, exports wacli/types and host-* interfaces)
    wac.push_str("// Host component (WASI bridge)\n");
    wac.push_str("let host = new wacli:host { ... };\n\n");

    // Instantiate each command plugin
    if !commands.is_empty() {
        wac.push_str("// Command plugins\n");
        for cmd in commands {
            let var_name = cmd.var_name();
            let pkg_name = cmd.package_name();
            wac.push_str(&format!(
                "let {var_name} = new {pkg_name} {{\n  types: host.types,\n  host-env: host.host-env,\n  host-io: host.host-io,\n  host-fs: host.host-fs,\n  host-process: host.host-process,\n  host-pipes: host.host-pipes,\n  ...\n}};\n\n",
            ));
        }
    }

    // Instantiate registry with all command exports
    wac.push_str("// Registry (command dispatch)\n");
    wac.push_str("let registry = new wacli:registry {\n");
    wac.push_str("  types: host.types");
    for cmd in commands {
        let var_name = cmd.var_name();
        wac.push_str(&format!(
            ",\n  {}-command: {}.command",
            cmd.name, var_name
        ));
    }
    wac.push_str("\n};\n\n");

    // Instantiate core
    wac.push_str("// Core (CLI router)\n");
    wac.push_str("let core = new wacli:core {\n");
    wac.push_str("  types: host.types,\n");
    wac.push_str("  host-env: host.host-env,\n");
    wac.push_str("  host-io: host.host-io,\n");
    wac.push_str("  host-process: host.host-process,\n");
    wac.push_str("  registry: registry.registry\n");
    wac.push_str("};\n\n");

    // Export run
    wac.push_str("// Export the CLI entry point\n");
    wac.push_str("export core.run;\n");

    wac
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_wac_empty_commands() {
        let wac = generate_wac("example:my-cli", &[]);
        assert!(wac.contains("package example:my-cli;"));
        assert!(wac.contains("let host = new wacli:host"));
        assert!(wac.contains("let registry = new wacli:registry"));
        assert!(wac.contains("let core = new wacli:core"));
        assert!(wac.contains("export core.run;"));
    }

    #[test]
    fn test_generate_wac_with_commands() {
        let commands = vec![
            CommandInfo {
                name: "greet".to_string(),
                path: PathBuf::from("commands/greet.component.wasm"),
            },
            CommandInfo {
                name: "hello-world".to_string(),
                path: PathBuf::from("commands/hello-world.component.wasm"),
            },
        ];

        let wac = generate_wac("example:hello-cli", &commands);

        assert!(wac.contains("package example:hello-cli;"));
        assert!(wac.contains("let greet = new wacli:cmd-greet"));
        assert!(wac.contains("let hello_world = new wacli:cmd-hello-world"));
        assert!(wac.contains("greet-command: greet.command"));
        assert!(wac.contains("hello-world-command: hello_world.command"));
        assert!(wac.contains("export core.run;"));
    }

    #[test]
    fn test_var_name_conversion() {
        let cmd = CommandInfo {
            name: "my-command".to_string(),
            path: PathBuf::from("test.wasm"),
        };
        assert_eq!(cmd.var_name(), "my_command");
    }
}
