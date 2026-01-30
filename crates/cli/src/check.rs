use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use wasmparser::{Payload, Parser};

#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub artifact: String,
    pub allowlist_source: String,
    pub allowlist: Vec<String>,
    pub imports: Vec<String>,
    pub extra_imports: Vec<String>,
    pub missing_imports: Vec<String>,
}

pub fn extract_imports(wasm_bytes: &[u8]) -> Result<Vec<String>> {
    let mut imports = Vec::new();
    let parser = Parser::new(0);
    let mut depth = 0;

    for payload in parser.parse_all(wasm_bytes) {
        let payload = payload.context("failed to parse WASM")?;

        match payload {
            // Track nesting depth for nested components/modules
            Payload::ModuleSection { .. } | Payload::ComponentSection { .. } => {
                depth += 1;
            }
            Payload::End(_) => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            // Only capture imports at the top level (depth == 0)
            Payload::ComponentImportSection(reader) if depth == 0 => {
                for import in reader {
                    let import = import.context("failed to read component import")?;
                    // import.name is a ComponentExternName which has .0 for the string
                    imports.push(import.name.0.to_string());
                }
            }
            _ => {}
        }
    }

    // Filter to only WIT-style imports (namespace:package/interface or namespace:package/interface@version)
    // This filters out internal imports like "import-func-*", "greeter", etc.
    let wit_imports: Vec<_> = imports
        .into_iter()
        .filter(|imp| {
            // WIT imports have format: namespace:package/interface[@version]
            imp.contains(':') && imp.contains('/')
        })
        .collect();

    // Deduplicate and sort
    let mut unique: Vec<_> = wit_imports.into_iter().collect::<HashSet<_>>().into_iter().collect();
    unique.sort();

    Ok(unique)
}

pub fn check_imports(
    wasm_path: &Path,
    allowlist: &[String],
    allowlist_source: &Path,
) -> Result<CheckReport> {
    let wasm_bytes = std::fs::read(wasm_path)
        .with_context(|| format!("failed to read WASM: {}", wasm_path.display()))?;

    let imports = extract_imports(&wasm_bytes)?;
    let allowed: HashSet<String> = allowlist.iter().cloned().collect();

    // Find extra imports (not in allowlist)
    let extra_imports: Vec<String> = imports
        .iter()
        .filter(|imp| !allowed.contains(*imp))
        .cloned()
        .collect();

    // Find missing imports (in allowlist but not used)
    let import_set: HashSet<_> = imports.iter().cloned().collect();
    let missing_imports: Vec<String> = allowed
        .iter()
        .filter(|a| !import_set.contains(*a))
        .cloned()
        .collect();

    let mut allowlist_sorted = allowlist.to_vec();
    allowlist_sorted.sort();

    Ok(CheckReport {
        artifact: wasm_path.display().to_string(),
        allowlist_source: allowlist_source.display().to_string(),
        allowlist: allowlist_sorted,
        imports,
        extra_imports,
        missing_imports,
    })
}
