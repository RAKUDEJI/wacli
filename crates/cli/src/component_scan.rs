//! Scan and validate command components in the commands/ directory.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use wasmparser::{Parser, Payload};

/// Information about a discovered command component.
#[derive(Debug, Clone)]
pub struct CommandInfo {
    /// Command name (derived from filename).
    pub name: String,
    /// Path to the component file.
    pub path: PathBuf,
}

impl CommandInfo {
    /// Returns a variable-safe name (hyphens replaced with underscores).
    pub fn var_name(&self) -> String {
        self.name.replace('-', "_")
    }

    /// Returns the package name for WAC composition.
    pub fn package_name(&self) -> String {
        format!("wacli:cmd-{}", self.name)
    }
}

/// Validate that a command name matches the required pattern: [a-z][a-z0-9-]*
fn is_valid_command_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();

    // First character must be lowercase letter
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }

    // Rest must be lowercase letters, digits, or hyphens
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            return false;
        }
    }

    // Cannot end with hyphen
    !name.ends_with('-')
}

/// Extract component exports from a WASM binary.
fn extract_exports(wasm_bytes: &[u8]) -> Result<Vec<String>> {
    let mut exports = Vec::new();
    let parser = Parser::new(0);
    let mut depth = 0;

    for payload in parser.parse_all(wasm_bytes) {
        let payload = payload.context("failed to parse WASM")?;

        match payload {
            Payload::ModuleSection { .. } | Payload::ComponentSection { .. } => {
                depth += 1;
            }
            Payload::End(_) => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            // Only capture exports at the top level (depth == 0)
            Payload::ComponentExportSection(reader) if depth == 0 => {
                for export in reader {
                    let export = export.context("failed to read component export")?;
                    exports.push(export.name.0.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(exports)
}

/// Check if a component exports the wacli:cli/command interface.
fn exports_command_interface(wasm_bytes: &[u8]) -> Result<bool> {
    let exports = extract_exports(wasm_bytes)?;
    // The export name should be "wacli:cli/command@1.0.0" or just "command"
    // depending on how the component was built
    Ok(exports
        .iter()
        .any(|e| e == "wacli:cli/command@1.0.0" || e == "wacli:cli/command" || e == "command"))
}

/// Scan the commands directory and return validated command info.
pub fn scan_commands(commands_dir: &Path) -> Result<Vec<CommandInfo>> {
    if !commands_dir.exists() {
        bail!("commands directory not found: {}", commands_dir.display());
    }

    if !commands_dir.is_dir() {
        bail!(
            "commands path is not a directory: {}",
            commands_dir.display()
        );
    }

    let mut commands = Vec::new();

    let entries = fs::read_dir(commands_dir).with_context(|| {
        format!(
            "failed to read commands directory: {}",
            commands_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry.context("failed to read directory entry")?;
        let path = entry.path();

        // Skip non-files
        if !path.is_file() {
            continue;
        }

        // Check for .component.wasm extension
        let file_name = path.file_name().unwrap().to_string_lossy();
        if !file_name.ends_with(".component.wasm") {
            continue;
        }

        // Extract command name from filename
        let name = file_name
            .strip_suffix(".component.wasm")
            .unwrap()
            .to_string();

        // Validate command name
        if !is_valid_command_name(&name) {
            bail!(
                "invalid command name '{}': must match pattern [a-z][a-z0-9-]* (file: {})",
                name,
                path.display()
            );
        }

        // Read and validate the component
        let wasm_bytes = fs::read(&path)
            .with_context(|| format!("failed to read component: {}", path.display()))?;

        if !exports_command_interface(&wasm_bytes)? {
            bail!(
                "'{}' does not export wacli:cli/command interface",
                path.display()
            );
        }

        commands.push(CommandInfo { name, path });
    }

    // Sort by name for deterministic output
    commands.sort_by(|a, b| a.name.cmp(&b.name));

    if commands.is_empty() {
        bail!("no commands found in {}", commands_dir.display());
    }

    Ok(commands)
}

/// Verify that required default components exist.
pub fn verify_defaults(defaults_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let host_path = defaults_dir.join("host.component.wasm");
    let core_path = defaults_dir.join("core.component.wasm");

    if !host_path.exists() {
        bail!("defaults/host.component.wasm not found");
    }

    if !core_path.exists() {
        bail!("defaults/core.component.wasm not found");
    }

    Ok((host_path, core_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_command_names() {
        assert!(is_valid_command_name("help"));
        assert!(is_valid_command_name("greet"));
        assert!(is_valid_command_name("my-command"));
        assert!(is_valid_command_name("cmd123"));
        assert!(is_valid_command_name("a"));
        assert!(is_valid_command_name("a1"));
        assert!(is_valid_command_name("hello-world-test"));
    }

    #[test]
    fn test_invalid_command_names() {
        assert!(!is_valid_command_name("")); // empty
        assert!(!is_valid_command_name("1cmd")); // starts with digit
        assert!(!is_valid_command_name("-cmd")); // starts with hyphen
        assert!(!is_valid_command_name("Cmd")); // uppercase
        assert!(!is_valid_command_name("CMD")); // all uppercase
        assert!(!is_valid_command_name("my_cmd")); // underscore
        assert!(!is_valid_command_name("cmd-")); // ends with hyphen
        assert!(!is_valid_command_name("my.cmd")); // dot
    }

    #[test]
    fn test_command_info_var_name() {
        let cmd = CommandInfo {
            name: "my-command".to_string(),
            path: PathBuf::from("test.wasm"),
        };
        assert_eq!(cmd.var_name(), "my_command");
    }

    #[test]
    fn test_command_info_package_name() {
        let cmd = CommandInfo {
            name: "greet".to_string(),
            path: PathBuf::from("test.wasm"),
        };
        assert_eq!(cmd.package_name(), "wacli:cmd-greet");
    }
}
