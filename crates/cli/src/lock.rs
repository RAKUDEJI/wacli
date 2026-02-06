use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_LOCK_NAME: &str = "wacli.lock";
pub const LOCK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockFile {
    pub schema_version: u32,

    /// Optional registry URL used when the lock was last updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub molt_registry: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<FrameworkLock>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<LockedRegistryCommand>,
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            schema_version: LOCK_SCHEMA_VERSION,
            molt_registry: None,
            framework: None,
            commands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkLock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<LockedComponent>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core: Option<LockedComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedComponent {
    pub repo: String,
    /// Tag or digest the user asked for (e.g. `v0.0.42`).
    pub reference: String,
    /// Resolved manifest digest (e.g. `sha256:...`). Used for deterministic pulls.
    pub digest: String,
    /// Digest of the selected WASM layer blob in the manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedRegistryCommand {
    pub name: String,
    pub repo: String,
    /// Tag or digest the user asked for (from `wacli.json`).
    pub reference: String,
    /// Resolved manifest digest (e.g. `sha256:...`). Used for deterministic pulls.
    pub digest: String,
    /// Digest of the selected WASM layer blob in the manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_digest: Option<String>,
}

pub fn lock_path(base_dir: &Path) -> PathBuf {
    base_dir.join(DEFAULT_LOCK_NAME)
}

pub fn load_lock(path: &Path) -> Result<Option<LockFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read lock file: {}", path.display()))?;
    let lock: LockFile = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse lock file JSON: {}", path.display()))?;
    if lock.schema_version != LOCK_SCHEMA_VERSION {
        bail!(
            "unsupported lock schemaVersion {} (expected {})",
            lock.schema_version,
            LOCK_SCHEMA_VERSION
        );
    }
    Ok(Some(lock))
}

pub fn write_lock(path: &Path, lock: &mut LockFile) -> Result<()> {
    if lock.schema_version != LOCK_SCHEMA_VERSION {
        bail!(
            "refusing to write lock with schemaVersion {} (expected {})",
            lock.schema_version,
            LOCK_SCHEMA_VERSION
        );
    }

    // Keep deterministic order for diffs.
    lock.commands.sort_by(|a, b| a.name.cmp(&b.name));

    let bytes = serde_json::to_vec_pretty(lock).context("failed to serialize lock file")?;
    let mut out = String::from_utf8(bytes).context("lock file is not valid UTF-8")?;
    out.push('\n');

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, out.as_bytes())
        .with_context(|| format!("failed to write {}", tmp.display()))?;
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    fs::rename(&tmp, path)
        .with_context(|| format!("failed to move {} into place", path.display()))?;
    Ok(())
}

impl LockFile {
    pub fn framework_host(&self) -> Option<&LockedComponent> {
        self.framework.as_ref().and_then(|f| f.host.as_ref())
    }

    pub fn framework_core(&self) -> Option<&LockedComponent> {
        self.framework.as_ref().and_then(|f| f.core.as_ref())
    }

    pub fn set_framework_host(&mut self, v: LockedComponent) {
        let f = self.framework.get_or_insert_with(FrameworkLock::default);
        f.host = Some(v);
    }

    pub fn set_framework_core(&mut self, v: LockedComponent) {
        let f = self.framework.get_or_insert_with(FrameworkLock::default);
        f.core = Some(v);
    }

    pub fn find_command(&self, name: &str) -> Option<&LockedRegistryCommand> {
        self.commands.iter().find(|c| c.name == name)
    }

    pub fn set_command(&mut self, v: LockedRegistryCommand) {
        if let Some(existing) = self.commands.iter_mut().find(|c| c.name == v.name) {
            *existing = v;
        } else {
            self.commands.push(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_round_trips() {
        let mut lock = LockFile::default();
        lock.molt_registry = Some("https://registry.example.com".to_string());
        lock.set_framework_host(LockedComponent {
            repo: "wacli/host".to_string(),
            reference: "v0.0.42".to_string(),
            digest: "sha256:deadbeef".to_string(),
            layer_digest: Some("sha256:cafebabe".to_string()),
        });
        lock.set_command(LockedRegistryCommand {
            name: "greet".to_string(),
            repo: "example/greet".to_string(),
            reference: "1.0.0".to_string(),
            digest: "sha256:abc".to_string(),
            layer_digest: None,
        });

        let json = serde_json::to_string_pretty(&lock).unwrap();
        let decoded: LockFile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.schema_version, LOCK_SCHEMA_VERSION);
        assert_eq!(
            decoded.molt_registry.as_deref(),
            Some("https://registry.example.com")
        );
        assert_eq!(decoded.framework_host().unwrap().digest, "sha256:deadbeef");
        assert_eq!(decoded.find_command("greet").unwrap().repo, "example/greet");
    }
}
