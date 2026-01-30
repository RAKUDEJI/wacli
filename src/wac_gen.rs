use crate::manifest::Manifest;

pub fn generate_wac(manifest: &Manifest) -> String {
    let mut wac = String::new();

    // Package declaration
    wac.push_str(&format!("package {};\n\n", manifest.package.name));

    // Instantiate host (imports WASI, exports wacli/types and wacli/host)
    wac.push_str("// Host component (WASI bridge)\n");
    wac.push_str("let host = new wacli-host { ... };\n\n");

    // Instantiate each command plugin
    wac.push_str("// Command plugins\n");
    for cmd in &manifest.command {
        let var_name = cmd.var_name();
        let pkg_name = cmd.package_name();
        wac.push_str(&format!(
            "let {var_name} = new {pkg_name} {{\n  types: host.types,\n  host: host.host\n}};\n\n",
        ));
    }

    // Instantiate registry with all command exports
    wac.push_str("// Registry (command dispatch)\n");
    wac.push_str("let registry = new example:hello-registry {\n");
    wac.push_str("  types: host.types");
    for cmd in &manifest.command {
        let var_name = cmd.var_name();
        wac.push_str(&format!(",\n  {}: {}.command", var_name, var_name));
    }
    wac.push_str("\n};\n\n");

    // Instantiate core
    wac.push_str("// Core (CLI router)\n");
    wac.push_str("let core = new wacli-core {\n");
    wac.push_str("  types: host.types,\n");
    wac.push_str("  host: host.host,\n");
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
    use crate::manifest::{Command, Framework, Output, Package};
    use std::path::PathBuf;

    #[test]
    fn test_generate_wac() {
        let manifest = Manifest {
            package: Package {
                name: "example:hello-cli".to_string(),
                version: Some("0.1.0".to_string()),
            },
            framework: Framework {
                host: PathBuf::from("host.component.wasm"),
                core: PathBuf::from("core.component.wasm"),
                registry: PathBuf::from("registry.component.wasm"),
            },
            command: vec![Command {
                name: "greet".to_string(),
                package: Some("example:greeter".to_string()),
                plugin: PathBuf::from("greeter.component.wasm"),
                aliases: vec![],
            }],
            output: Some(Output {
                path: PathBuf::from("output.wasm"),
            }),
        };

        let wac = generate_wac(&manifest);
        assert!(wac.contains("package example:hello-cli;"));
        assert!(wac.contains("let greeter = new example:greeter"));
        assert!(wac.contains("export core.run;"));
    }
}
