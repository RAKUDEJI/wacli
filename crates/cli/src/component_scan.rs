//! Scan and validate command components in the commands/ directory.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use wasmparser::{Parser, Payload};
use wacli_metadata::CommandMetadataV1;

use crate::command_metadata::extract_command_metadata;

/// Information about a discovered command component.
#[derive(Debug, Clone)]
pub struct CommandInfo {
    /// Command name (derived from filename).
    pub name: String,
    /// Path to the component file.
    pub path: PathBuf,
    /// Top-level component import names.
    pub imports: Vec<String>,
    /// Embedded command metadata (extracted from a custom section).
    pub metadata: CommandMetadataV1,
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

    /// Resolve the preferred import name for a given base (e.g. "host-env").
    /// Falls back to the fully qualified name if no match is found.
    pub fn import_name(&self, base: &str) -> String {
        let fqn = format!("wacli:cli/{base}@2.0.0");
        if self.imports.iter().any(|i| i == &fqn) {
            return fqn;
        }
        let pkg = format!("wacli:cli/{base}");
        if self.imports.iter().any(|i| i == &pkg) {
            return pkg;
        }
        if self.imports.iter().any(|i| i == base) {
            return base.to_string();
        }
        fqn
    }
}

/// Validate that a command name matches the required pattern: [a-z][a-z0-9-]*
pub fn is_valid_command_name(name: &str) -> bool {
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

/// Inspect a single `*.component.wasm` file and return validated command info.
///
/// This is useful when the command component was resolved outside of `commandsDir`
/// (e.g. pulled from a registry cache).
pub fn inspect_command_component(path: &Path) -> Result<CommandInfo> {
    if !path.exists() {
        bail!("command component not found: {}", path.display());
    }
    if !path.is_file() {
        bail!("command component path is not a file: {}", path.display());
    }

    let file_name = path
        .file_name()
        .context("command component path has no filename")?
        .to_string_lossy();
    if !file_name.ends_with(".component.wasm") {
        bail!(
            "command component filename must end with .component.wasm: {}",
            path.display()
        );
    }

    let name = file_name
        .strip_suffix(".component.wasm")
        .unwrap()
        .to_string();
    if !is_valid_command_name(&name) {
        bail!(
            "invalid command name '{}': must match pattern [a-z][a-z0-9-]* (file: {})",
            name,
            path.display()
        );
    }

    let wasm_bytes =
        fs::read(path).with_context(|| format!("failed to read component: {}", path.display()))?;

    let (exports, imports) = match analyze_wasm(&wasm_bytes)? {
        WasmKind::CoreModule => {
            bail!(
                "'{}' is a core WebAssembly module, not a component.\n\
                 Hint: run `wasm-tools component new {} -o {}`",
                path.display(),
                path.display(),
                path.display()
            );
        }
        WasmKind::Component { exports, imports } => (exports, imports),
    };

    if !exports_command_interface(&exports) {
        bail!(
            "'{}' does not export wacli:cli/command interface",
            path.display()
        );
    }

    let Some(metadata) = extract_command_metadata(&wasm_bytes)
        .with_context(|| format!("failed to extract command metadata from {}", path.display()))?
    else {
        bail!(
            "missing embedded command metadata in {}\n\
\n\
Expected a WASM custom section named '{}'.\n\
\n\
Fix:\n\
- Update your plugin to use `wacli_cdk::declare_command_metadata!(...)` (and rebuild the component).",
            path.display(),
            wacli_metadata::COMMAND_METADATA_SECTION
        );
    };

    if metadata.command_meta.name != name {
        bail!(
            "command metadata name mismatch for {}\n\
\n\
file name: {}\n\
meta.name:  {}\n\
\n\
The command name is derived from the `.component.wasm` file name and must match `meta.name`.",
            path.display(),
            name,
            metadata.command_meta.name
        );
    }

    if let Some(schema) = metadata.command_schema.as_ref()
        && schema.name != name
    {
        bail!(
            "command schema name mismatch for {}\n\
\n\
file name:   {}\n\
schema.name: {}\n\
\n\
The command name is derived from the `.component.wasm` file name and must match `schema.name`.",
            path.display(),
            name,
            schema.name
        );
    }

    Ok(CommandInfo {
        name,
        path: path.to_path_buf(),
        imports,
        metadata,
    })
}

/// Result of analyzing a WASM binary.
enum WasmKind {
    /// A WebAssembly Component with its exports.
    Component {
        exports: Vec<String>,
        imports: Vec<String>,
    },
    /// A core WebAssembly module (not a component).
    CoreModule,
}

/// Analyze a WASM binary to determine its kind and extract exports.
fn analyze_wasm(wasm_bytes: &[u8]) -> Result<WasmKind> {
    let parser = Parser::new(0);
    let mut is_component = false;
    let mut exports = Vec::new();
    let mut imports = Vec::new();
    let mut depth = 0;

    for payload in parser.parse_all(wasm_bytes) {
        let payload = payload.context("failed to parse WASM")?;

        match payload {
            Payload::Version { encoding, .. } => {
                is_component = encoding == wasmparser::Encoding::Component;
            }
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
            Payload::ComponentImportSection(reader) if depth == 0 => {
                for import in reader {
                    let import = import.context("failed to read component import")?;
                    imports.push(import.name.0.to_string());
                }
            }
            _ => {}
        }
    }

    if is_component {
        Ok(WasmKind::Component { exports, imports })
    } else {
        Ok(WasmKind::CoreModule)
    }
}

/// Check if a component exports the wacli:cli/command interface.
fn exports_command_interface(exports: &[String]) -> bool {
    exports
        .iter()
        .any(|e| e == "wacli:cli/command@2.0.0" || e == "wacli:cli/command" || e == "command")
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
    let mut seen = HashMap::new();

    collect_commands(commands_dir, &mut commands, &mut seen)?;

    // Sort by name for deterministic output
    commands.sort_by(|a, b| a.name.cmp(&b.name));

    if commands.is_empty() {
        bail!("no commands found in {}", commands_dir.display());
    }

    Ok(commands)
}

/// Scan the commands directory if it exists.
///
/// Unlike `scan_commands`, this returns an empty list when the directory is
/// missing or contains no `*.component.wasm` files.
pub fn scan_commands_optional(commands_dir: &Path) -> Result<Vec<CommandInfo>> {
    if !commands_dir.exists() {
        return Ok(Vec::new());
    }

    if !commands_dir.is_dir() {
        bail!(
            "commands path is not a directory: {}",
            commands_dir.display()
        );
    }

    let mut commands = Vec::new();
    let mut seen = HashMap::new();
    collect_commands(commands_dir, &mut commands, &mut seen)?;

    // Sort by name for deterministic output
    commands.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(commands)
}

fn collect_commands(
    dir: &Path,
    out: &mut Vec<CommandInfo>,
    seen: &mut HashMap<String, PathBuf>,
) -> Result<()> {
    let entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read commands directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry.context("failed to read directory entry")?;
        let path = entry.path();

        if path.is_dir() {
            collect_commands(&path, out, seen)?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name() {
            Some(name) => name.to_string_lossy(),
            None => continue,
        };
        if !file_name.ends_with(".component.wasm") {
            continue;
        }

        let name = file_name
            .strip_suffix(".component.wasm")
            .unwrap()
            .to_string();

        if !is_valid_command_name(&name) {
            bail!(
                "invalid command name '{}': must match pattern [a-z][a-z0-9-]* (file: {})",
                name,
                path.display()
            );
        }

        if let Some(prev) = seen.get(&name) {
            bail!(
                "duplicate command name '{}':\n  {}\n  {}",
                name,
                prev.display(),
                path.display()
            );
        }
        seen.insert(name.clone(), path.clone());

        let wasm_bytes = fs::read(&path)
            .with_context(|| format!("failed to read component: {}", path.display()))?;

        let (exports, imports) = match analyze_wasm(&wasm_bytes)? {
            WasmKind::CoreModule => {
                bail!(
                    "'{}' is a core WebAssembly module, not a component.\n\
                     Hint: run `wasm-tools component new {} -o {}`",
                    path.display(),
                    path.display(),
                    path.display()
                );
            }
            WasmKind::Component { exports, imports } => (exports, imports),
        };

        if !exports_command_interface(&exports) {
            bail!(
                "'{}' does not export wacli:cli/command interface",
                path.display()
            );
        }

        let Some(metadata) = extract_command_metadata(&wasm_bytes)
            .with_context(|| format!("failed to extract command metadata from {}", path.display()))?
        else {
            bail!(
                "missing embedded command metadata in {}\n\
\n\
Expected a WASM custom section named '{}'.\n\
\n\
Fix:\n\
- Update your plugin to use `wacli_cdk::declare_command_metadata!(...)` (and rebuild the component).",
                path.display(),
                wacli_metadata::COMMAND_METADATA_SECTION
            );
        };

        if metadata.command_meta.name != name {
            bail!(
                "command metadata name mismatch for {}\n\
\n\
file name: {}\n\
meta.name:  {}\n\
\n\
The command name is derived from the `.component.wasm` file name and must match `meta.name`.",
                path.display(),
                name,
                metadata.command_meta.name
            );
        }

        if let Some(schema) = metadata.command_schema.as_ref()
            && schema.name != name
        {
            bail!(
                "command schema name mismatch for {}\n\
\n\
file name:   {}\n\
schema.name: {}\n\
\n\
The command name is derived from the `.component.wasm` file name and must match `schema.name`.",
                path.display(),
                name,
                schema.name
            );
        }

        out.push(CommandInfo {
            name,
            path,
            imports,
            metadata,
        });
    }

    Ok(())
}

/// Verify that required default components exist.
pub fn verify_defaults(defaults_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let host_path = defaults_dir.join("host.component.wasm");
    let core_path = defaults_dir.join("core.component.wasm");

    if !host_path.exists() {
        bail!("host.component.wasm not found: {}", host_path.display());
    }

    if !core_path.exists() {
        bail!("core.component.wasm not found: {}", core_path.display());
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
            imports: Vec::new(),
            metadata: CommandMetadataV1 {
                format_version: 1,
                command_meta: wacli_metadata::CommandMeta {
                    name: "my-command".to_string(),
                    ..Default::default()
                },
                command_schema: None,
            },
        };
        assert_eq!(cmd.var_name(), "my_command");
    }

    #[test]
    fn test_command_info_package_name() {
        let cmd = CommandInfo {
            name: "greet".to_string(),
            path: PathBuf::from("test.wasm"),
            imports: Vec::new(),
            metadata: CommandMetadataV1 {
                format_version: 1,
                command_meta: wacli_metadata::CommandMeta {
                    name: "greet".to_string(),
                    ..Default::default()
                },
                command_schema: None,
            },
        };
        assert_eq!(cmd.package_name(), "wacli:cmd-greet");
    }
}
