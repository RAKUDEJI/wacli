//! Molt WASM-aware registry client.
//!
//! This crate is a thin, WASM-focused extension on top of [`oci-client`]
//! (oras-project/rust-oci-client).
//!
//! It supports:
//! - OCI Distribution API (`/v2`) helpers for pulling component wasm layers.
//! - Molt extension API (`/wasm/v1`) helpers for WIT, interface index, and search.
//!
//! The Molt spec this targets is described in this repo's docs:
//! - WIT referrers: `application/vnd.wasm.wit.v1+text`
//! - `/wasm/v1` endpoints: `.../wit`, `.../interfaces`, `.../dependencies`, `/search`
//!
//! # Example (env-configured)
//!
//! ```no_run
//! use molt_registry_client::{OciWasmClient, WasmV1Client, WitRequest};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let oci = OciWasmClient::from_env()?.expect("set MOLT_REGISTRY");
//! let wasm = oci.pull_component_wasm("example/repo", "1.0.0").await?;
//! println!("downloaded {} bytes", wasm.len());
//!
//! let wasm_v1 = WasmV1Client::from_env()?.expect("set MOLT_REGISTRY");
//! let wit = wasm_v1
//!     .wit_text("example/repo", "1.0.0", &WitRequest::default())
//!     .await?;
//! println!("{}", wit.text);
//!
//! # Ok(()) }
//! ```

mod media_types;
mod oci;
mod util;
mod wasm_v1;

pub use media_types::*;
pub use oci::*;
pub use util::RegistryEndpoint;
pub use util::{auth_from_env, auth_from_header_line, sanitize_path_segment};
pub use wasm_v1::*;
