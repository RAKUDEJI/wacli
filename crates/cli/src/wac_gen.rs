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
    let host_env_import = "\"wacli:cli/host-env@1.0.0\"";
    let host_io_import = "\"wacli:cli/host-io@1.0.0\"";
    let host_process_import = "\"wacli:cli/host-process@1.0.0\"";
    let registry_import = "\"wacli:cli/registry@1.0.0\"";
    let types_import = "\"wacli:cli/types@1.0.0\"";

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
            let cmd_types_import = cmd.import_name("types");
            let cmd_host_env_import = cmd.import_name("host-env");
            let cmd_host_io_import = cmd.import_name("host-io");
            let cmd_host_fs_import = cmd.import_name("host-fs");
            let cmd_host_process_import = cmd.import_name("host-process");
            let cmd_host_pipes_import = cmd.import_name("host-pipes");
            wac.push_str(&format!(
                "let {var_name} = new {pkg_name} {{\n  \"{cmd_types_import}\": host.types,\n  \"{cmd_host_env_import}\": host.host-env,\n  \"{cmd_host_io_import}\": host.host-io,\n  \"{cmd_host_fs_import}\": host.host-fs,\n  \"{cmd_host_process_import}\": host.host-process,\n  \"{cmd_host_pipes_import}\": host.host-pipes,\n  ...\n}};\n\n",
            ));
        }
    }

    // Instantiate registry with all command exports
    wac.push_str("// Registry (command dispatch)\n");
    wac.push_str("let registry = new wacli:registry {\n");
    wac.push_str(&format!("  {types_import}: host.types"));
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
    wac.push_str(&format!("  {types_import}: host.types,\n"));
    wac.push_str(&format!("  {host_env_import}: host.host-env,\n"));
    wac.push_str(&format!("  {host_io_import}: host.host-io,\n"));
    wac.push_str(&format!("  {host_process_import}: host.host-process,\n"));
    wac.push_str(&format!("  {registry_import}: registry.registry\n"));
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
                imports: Vec::new(),
            },
            CommandInfo {
                name: "hello-world".to_string(),
                path: PathBuf::from("commands/hello-world.component.wasm"),
                imports: Vec::new(),
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
            imports: Vec::new(),
        };
        assert_eq!(cmd.var_name(), "my_command");
    }
}
