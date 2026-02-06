use anyhow::{Context, Result};
use molt_registry_client::OciWasmClient;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PulledComponentDigests {
    pub manifest_digest: String,
    pub layer_digest: String,
}

pub fn pull_component_wasm_to_file(
    client: &OciWasmClient,
    repo: &str,
    reference: &str,
    dest: &Path,
    overwrite: bool,
) -> Result<()> {
    let _ = pull_component_wasm_to_file_with_digests(client, repo, reference, dest, overwrite)?;
    Ok(())
}

pub fn pull_component_wasm_to_file_with_digests(
    client: &OciWasmClient,
    repo: &str,
    reference: &str,
    dest: &Path,
    overwrite: bool,
) -> Result<Option<PulledComponentDigests>> {
    if dest.exists() && !overwrite {
        return Ok(None);
    }

    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to initialize async runtime")?;

    let pulled = rt
        .block_on(client.pull_component_wasm_with_digests(repo, reference))
        .with_context(|| format!("failed to pull component from registry: {repo}:{reference}"))?;
    let bytes = pulled.bytes;

    let tmp = dest.with_extension("download");
    fs::write(&tmp, &bytes).with_context(|| format!("failed to write {}", tmp.display()))?;
    if overwrite && dest.exists() {
        fs::remove_file(dest).with_context(|| format!("failed to remove {}", dest.display()))?;
    }
    fs::rename(&tmp, dest).with_context(|| {
        format!(
            "failed to move {} into place at {}",
            tmp.display(),
            dest.display()
        )
    })?;

    Ok(Some(PulledComponentDigests {
        manifest_digest: pulled.manifest_digest,
        layer_digest: pulled.layer_digest,
    }))
}
