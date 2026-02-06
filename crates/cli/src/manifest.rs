use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_MANIFEST_NAME: &str = "wacli.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildManifest>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<PathBuf>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "defaults_dir"
    )]
    pub defaults_dir: Option<PathBuf>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "commands_dir"
    )]
    pub commands_dir: Option<PathBuf>,

    /// Optional list of command plugins to pull from an OCI registry.
    ///
    /// Each entry resolves to a `<name>.component.wasm` and is treated the same
    /// as a file found under `commandsDir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<RegistryCommand>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryCommand {
    /// Command name (must match [a-z][a-z0-9-]*)
    pub name: String,
    /// OCI repository name (may include '/')
    pub repo: String,
    /// Tag or manifest digest
    pub reference: String,
}

#[derive(Debug, Clone)]
pub struct LoadedManifest {
    pub base_dir: PathBuf,
    pub manifest: Manifest,
}

pub fn load_manifest(manifest_path: Option<&Path>) -> Result<Option<LoadedManifest>> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;

    let (path, explicit) = match manifest_path {
        Some(p) => (resolve_against(&cwd, p), true),
        None => (cwd.join(DEFAULT_MANIFEST_NAME), false),
    };

    if !path.exists() {
        if explicit {
            bail!("manifest not found: {}", path.display());
        }
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read manifest: {}", path.display()))?;
    let manifest: Manifest = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse manifest JSON: {}", path.display()))?;

    let base_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or(cwd);

    Ok(Some(LoadedManifest { base_dir, manifest }))
}

pub fn write_default_manifest(project_dir: &Path, overwrite: bool) -> Result<PathBuf> {
    let dest = project_dir.join(DEFAULT_MANIFEST_NAME);
    if dest.exists() && !overwrite {
        return Ok(dest);
    }

    let project_name = guess_project_name(project_dir).unwrap_or_else(|| "my-cli".to_string());

    let manifest = Manifest {
        schema_version: Some(1),
        build: Some(BuildManifest {
            name: Some(format!("example:{project_name}")),
            version: Some("0.1.0".to_string()),
            output: Some(PathBuf::from(format!("{project_name}.component.wasm"))),
            defaults_dir: Some(PathBuf::from("defaults")),
            commands_dir: Some(PathBuf::from("commands")),
            commands: None,
        }),
    };

    let bytes = serde_json::to_vec_pretty(&manifest).context("failed to serialize manifest")?;
    let mut out = String::from_utf8(bytes).context("manifest is not valid UTF-8")?;
    out.push('\n');

    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, out.as_bytes())
        .with_context(|| format!("failed to write {}", tmp.display()))?;
    if overwrite && dest.exists() {
        fs::remove_file(&dest).with_context(|| format!("failed to remove {}", dest.display()))?;
    }
    fs::rename(&tmp, &dest)
        .with_context(|| format!("failed to move {} into place", dest.display()))?;
    Ok(dest)
}

fn resolve_against(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn guess_project_name(project_dir: &Path) -> Option<String> {
    // For `.` or other non-meaningful paths, try the current directory name.
    let file_name = project_dir.file_name().and_then(|s| s.to_str());
    let direct = file_name.filter(|s| !s.is_empty() && *s != "." && *s != "..");
    if let Some(name) = direct {
        return Some(name.to_string());
    }

    let cwd = std::env::current_dir().ok()?;
    cwd.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("wacli-{prefix}-{pid}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn manifest_deserializes_camel_case() {
        let json = r#"{
  "schemaVersion": 1,
  "build": {
    "name": "example:demo",
    "version": "0.1.0",
    "output": "out.component.wasm",
    "defaultsDir": "defaults",
    "commandsDir": "commands",
    "commands": [
      { "name": "greet", "repo": "example/greet", "reference": "1.0.0" }
    ]
  }
}"#;
        let m: Manifest = serde_json::from_str(json).unwrap();
        let build = m.build.unwrap();
        assert_eq!(m.schema_version, Some(1));
        assert_eq!(build.name.as_deref(), Some("example:demo"));
        assert_eq!(build.version.as_deref(), Some("0.1.0"));
        assert_eq!(
            build.output.as_deref(),
            Some(Path::new("out.component.wasm"))
        );
        assert_eq!(build.defaults_dir.as_deref(), Some(Path::new("defaults")));
        assert_eq!(build.commands_dir.as_deref(), Some(Path::new("commands")));
        let cmds = build.commands.unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].name, "greet");
        assert_eq!(cmds[0].repo, "example/greet");
        assert_eq!(cmds[0].reference, "1.0.0");
    }

    #[test]
    fn write_default_manifest_writes_expected_defaults() {
        let dir = make_temp_dir("manifest-defaults");
        let dest = write_default_manifest(&dir, false).unwrap();
        let contents = fs::read_to_string(&dest).unwrap();
        let m: Manifest = serde_json::from_str(&contents).unwrap();
        assert_eq!(m.schema_version, Some(1));

        let build = m.build.unwrap();
        let project_name = dir.file_name().unwrap().to_string_lossy();
        assert_eq!(build.name, Some(format!("example:{project_name}")));
        assert_eq!(build.version.as_deref(), Some("0.1.0"));
        assert_eq!(
            build.output,
            Some(PathBuf::from(format!("{project_name}.component.wasm")))
        );
        assert_eq!(build.defaults_dir.as_deref(), Some(Path::new("defaults")));
        assert_eq!(build.commands_dir.as_deref(), Some(Path::new("commands")));

        let _ = fs::remove_dir_all(&dir);
    }
}
