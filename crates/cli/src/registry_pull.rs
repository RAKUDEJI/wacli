use anyhow::{Context, Result};
use molt_registry_client::OciWasmClient;
use std::fs;
use std::path::Path;

pub fn pull_component_wasm_to_file(
    client: &OciWasmClient,
    repo: &str,
    reference: &str,
    dest: &Path,
    overwrite: bool,
) -> Result<()> {
    if dest.exists() && !overwrite {
        return Ok(());
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

    let bytes = rt
        .block_on(client.pull_component_wasm(repo, reference))
        .with_context(|| format!("failed to pull component from registry: {repo}:{reference}"))?;

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
    Ok(())
}
