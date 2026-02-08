//! Shared metadata model for wacli command components.
//!
//! This crate intentionally does **not** depend on `wit-bindgen` bindings.
//! The data types here mirror the WIT records in `wacli:cli/types@2.0.0`,
//! and are used for:
//! - embedding metadata into a WASM custom section (no plugin execution)
//! - extracting metadata during `wacli build` (registry generation)

use serde::{Deserialize, Serialize};

/// Custom section name containing JSON-encoded command metadata.
///
/// The payload is a JSON object `CommandMetadataV1`.
pub const COMMAND_METADATA_SECTION: &str = "wacli:cli/command-metadata@1";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ArgDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_name: Option<String>,
    #[serde(default)]
    pub takes_value: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct CommandMeta {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub usage: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<ArgDef>,
}

/// Richer command schema (optional in the v1 payload).
///
/// This is currently a superset of `CommandMeta` with additional per-arg semantics
/// such as env, possible values, and conflict rules.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ArgSchema {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_name: Option<String>,
    #[serde(default)]
    pub takes_value: bool,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub possible_values: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct CommandSchema {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub usage: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<ArgSchema>,
}

impl CommandSchema {
    pub fn from_meta(meta: &CommandMeta) -> Self {
        Self {
            name: meta.name.clone(),
            summary: meta.summary.clone(),
            usage: meta.usage.clone(),
            aliases: meta.aliases.clone(),
            version: meta.version.clone(),
            hidden: meta.hidden,
            description: meta.description.clone(),
            examples: meta.examples.clone(),
            args: meta
                .args
                .iter()
                .map(|a| ArgSchema {
                    name: a.name.clone(),
                    short: a.short.clone(),
                    long: a.long.clone(),
                    help: a.help.clone(),
                    required: a.required,
                    default_value: a.default_value.clone(),
                    env: None,
                    value_name: a.value_name.clone(),
                    takes_value: a.takes_value,
                    // Preserve existing behavior: allow repeated occurrences unless specified.
                    multiple: true,
                    value_type: None,
                    possible_values: Vec::new(),
                    conflicts_with: Vec::new(),
                    requires: Vec::new(),
                    hidden: false,
                })
                .collect(),
        }
    }
}

/// JSON payload embedded into the `COMMAND_METADATA_SECTION` custom section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CommandMetadataV1 {
    pub format_version: u32,
    pub command_meta: CommandMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_schema: Option<CommandSchema>,
}

impl CommandMetadataV1 {
    pub fn new(command_meta: CommandMeta, command_schema: Option<CommandSchema>) -> Self {
        Self {
            format_version: 1,
            command_meta,
            command_schema,
        }
    }

    /// Encode as JSON bytes for embedding.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        // Deterministic formatting isn't required; we aim for a stable structure,
        // not stable whitespace.
        serde_json::to_vec(self).unwrap_or_default()
    }
}
