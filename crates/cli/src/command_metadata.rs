use anyhow::{Context, Result, bail};
use wasmparser::{Parser, Payload};

use wacli_metadata::{COMMAND_METADATA_SECTION, CommandMetadataV1};

pub fn extract_command_metadata(component_bytes: &[u8]) -> Result<Option<CommandMetadataV1>> {
    let Some(raw) = find_custom_section_in_component(component_bytes, COMMAND_METADATA_SECTION)?
    else {
        return Ok(None);
    };

    let meta: CommandMetadataV1 =
        serde_json::from_slice(&raw).context("failed to parse command metadata JSON")?;

    if meta.format_version != 1 {
        bail!(
            "unsupported command metadata format-version {} (expected 1)",
            meta.format_version
        );
    }

    Ok(Some(meta))
}

fn find_custom_section_in_component(bytes: &[u8], section_name: &str) -> Result<Option<Vec<u8>>> {
    // `Parser::parse_all` automatically descends into nested modules/components
    // and yields their payloads as well, so scanning for `CustomSection` is
    // sufficient here.
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.context("failed to parse WASM")?;
        let Payload::CustomSection(reader) = payload else {
            continue;
        };
        if reader.name() == section_name {
            return Ok(Some(reader.data().to_vec()));
        }
    }

    Ok(None)
}
