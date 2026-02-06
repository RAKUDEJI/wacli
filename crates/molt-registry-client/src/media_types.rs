//! Media types and artifact types used by Molt's WASM-aware registry extension.

/// Component artifactType (recommended).
pub const WASM_COMPONENT_ARTIFACT_TYPE: &str = "application/vnd.wasm.component.v1+wasm";

/// Component layer media type (existing / used by Molt).
pub const WASM_COMPONENT_LAYER_MEDIA_TYPE: &str = "application/wasm";

/// Molt config media type used by component artifacts (v0).
pub const WASM_CONFIG_MEDIA_TYPE_V0: &str = "application/vnd.wasm.config.v0+json";

/// WIT referrer artifactType (v1).
pub const WIT_ARTIFACT_TYPE_V1: &str = "application/vnd.wasm.wit.v1+text";

/// WIT layer media type (v1).
pub const WIT_LAYER_MEDIA_TYPE_V1: &str = "application/vnd.wasm.wit.v1+text";

/// Recommended media type for empty `{}` config blobs for artifact manifests.
pub const OCI_EMPTY_CONFIG_MEDIA_TYPE: &str = "application/vnd.oci.empty.v1+json";

/// OCI image manifest media type (required for WIT referrers).
pub const OCI_IMAGE_MANIFEST_MEDIA_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";
