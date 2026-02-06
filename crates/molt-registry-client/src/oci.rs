use crate::media_types::{
    OCI_EMPTY_CONFIG_MEDIA_TYPE, OCI_IMAGE_MANIFEST_MEDIA_TYPE, WASM_COMPONENT_LAYER_MEDIA_TYPE,
    WIT_ARTIFACT_TYPE_V1, WIT_LAYER_MEDIA_TYPE_V1,
};
use crate::util::{RegistryEndpoint, auth_from_env};
use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use oci_client::client::ClientConfig;
use oci_client::client::{Config, ImageLayer};
use oci_client::manifest::OciDescriptor;
use oci_client::manifest::OciImageManifest;
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, Reference};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct OciWasmClient {
    endpoint: RegistryEndpoint,
    auth: RegistryAuth,
    client: Client,
}

impl OciWasmClient {
    pub fn new(endpoint: RegistryEndpoint, auth: RegistryAuth) -> Result<Self> {
        let mut cfg = ClientConfig::default();
        cfg.user_agent = concat!("molt-registry-client/", env!("CARGO_PKG_VERSION"));
        let client = Client::try_from(cfg).context("failed to create oci-client")?;

        Ok(Self {
            endpoint,
            auth,
            client,
        })
    }

    pub fn from_env() -> Result<Option<Self>> {
        let Ok(base_url) = std::env::var("MOLT_REGISTRY") else {
            return Ok(None);
        };
        if base_url.trim().is_empty() {
            return Ok(None);
        }

        let endpoint = RegistryEndpoint::parse(&base_url)?;
        let auth = auth_from_env()?;
        Ok(Some(Self::new(endpoint, auth)?))
    }

    pub fn endpoint(&self) -> &RegistryEndpoint {
        &self.endpoint
    }

    pub fn auth(&self) -> &RegistryAuth {
        &self.auth
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn reference(&self, repo: &str, reference: &str) -> Result<Reference> {
        let repo = repo.trim_matches('/').to_string();
        let reference = reference.trim_matches('/').to_string();
        if repo.is_empty() {
            bail!("repo must not be empty");
        }
        if reference.is_empty() {
            bail!("reference must not be empty");
        }

        let reg = self.endpoint.registry.clone();
        if reference.starts_with("sha256:") || reference.starts_with("sha512:") {
            Ok(Reference::with_digest(reg, repo, reference))
        } else {
            Ok(Reference::with_tag(reg, repo, reference))
        }
    }

    pub async fn pull_image_manifest(
        &self,
        repo: &str,
        reference: &str,
    ) -> Result<(OciImageManifest, String)> {
        let r = self.reference(repo, reference)?;
        self.client
            .pull_image_manifest(&r, &self.auth)
            .await
            .context("failed to pull manifest")
    }

    pub async fn pull_manifest_and_config_json(
        &self,
        repo: &str,
        reference: &str,
    ) -> Result<(OciImageManifest, String, Value)> {
        let r = self.reference(repo, reference)?;
        let (manifest, digest, config) = self
            .client
            .pull_manifest_and_config(&r, &self.auth)
            .await
            .context("failed to pull manifest+config")?;
        let config_json: Value =
            serde_json::from_str(&config).context("config is not valid JSON")?;
        Ok((manifest, digest, config_json))
    }

    /// Pull the component wasm bytes for the given repo+reference.
    ///
    /// This selects the first layer matching:
    /// - `application/wasm` (Molt)
    /// - `application/vnd.wasm.content.layer.v1+wasm` (oci-client default constant)
    ///
    /// If the manifest has exactly one layer, that layer is used as a fallback.
    pub async fn pull_component_wasm(&self, repo: &str, reference: &str) -> Result<Vec<u8>> {
        let (manifest, _digest) = self.pull_image_manifest(repo, reference).await?;

        if manifest.layers.is_empty() {
            bail!("manifest has no layers");
        }

        let pick = if manifest.layers.len() == 1 {
            &manifest.layers[0]
        } else {
            manifest
                .layers
                .iter()
                .find(|l| {
                    l.media_type == WASM_COMPONENT_LAYER_MEDIA_TYPE
                        || l.media_type == oci_client::manifest::WASM_LAYER_MEDIA_TYPE
                })
                .context("no wasm layer found in manifest")?
        };

        let r = self.reference(repo, reference)?;
        pull_blob_to_bytes(&self.client, &r, pick).await
    }

    /// Push a WIT referrer artifact (OCI Referrers) for the given component subject.
    ///
    /// `subject_reference` can be a tag or digest; the method resolves it to a
    /// subject digest and uses that in the referrer manifest's `subject` field.
    ///
    /// Note: The referrer itself is pushed under `wit_tag` in the same repository.
    pub async fn push_wit_referrer(
        &self,
        repo: &str,
        subject_reference: &str,
        wit_tag: &str,
        wit_text: &str,
        file_name: &str,
        package: Option<&str>,
        generator: Option<&str>,
    ) -> Result<PushWitResult> {
        const ACCEPT_MANIFEST: [&str; 4] = [
            "application/vnd.oci.image.manifest.v1+json",
            "application/vnd.oci.image.index.v1+json",
            "application/vnd.docker.distribution.manifest.v2+json",
            "application/vnd.docker.distribution.manifest.list.v2+json",
        ];

        let repo = repo.trim_matches('/').to_string();
        if repo.is_empty() {
            bail!("repo must not be empty");
        }
        let wit_tag = wit_tag.trim();
        if wit_tag.is_empty() {
            bail!("wit_tag must not be empty");
        }

        // Pull the subject manifest raw bytes to get size + digest.
        let subject_ref = self.reference(&repo, subject_reference)?;
        let (subject_bytes, subject_digest) = self
            .client
            .pull_manifest_raw(&subject_ref, &self.auth, &ACCEPT_MANIFEST)
            .await
            .context("failed to pull subject manifest")?;

        let subject_size = subject_bytes.len() as i64;
        let subject_media_type = serde_json::from_slice::<Value>(&subject_bytes)
            .ok()
            .and_then(|v| {
                v.get("mediaType")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| OCI_IMAGE_MANIFEST_MEDIA_TYPE.to_string());

        let subject = OciDescriptor {
            media_type: subject_media_type,
            digest: subject_digest.clone(),
            size: subject_size,
            urls: None,
            annotations: None,
        };

        // Layer: WIT text
        let mut layer_annotations = BTreeMap::new();
        layer_annotations.insert(
            "org.opencontainers.image.title".to_string(),
            file_name.to_string(),
        );
        let layers = vec![ImageLayer::new(
            wit_text.as_bytes().to_vec(),
            WIT_LAYER_MEDIA_TYPE_V1.to_string(),
            Some(layer_annotations),
        )];

        // Config: empty json
        let config = Config::new(
            b"{}".to_vec(),
            OCI_EMPTY_CONFIG_MEDIA_TYPE.to_string(),
            None,
        );

        // Manifest annotations
        let mut manifest_annotations = BTreeMap::new();
        let generator = generator.unwrap_or("molt-registry-client");
        manifest_annotations.insert("dev.molt.wit.generator".to_string(), generator.to_string());
        if let Some(package) = package {
            let package = package.trim();
            if !package.is_empty() {
                manifest_annotations
                    .insert("dev.molt.wit.package".to_string(), package.to_string());
            }
        }

        let mut manifest = oci_client::manifest::OciImageManifest::build(
            &layers,
            &config,
            Some(manifest_annotations),
        );
        manifest.media_type = Some(OCI_IMAGE_MANIFEST_MEDIA_TYPE.to_string());
        manifest.artifact_type = Some(WIT_ARTIFACT_TYPE_V1.to_string());
        manifest.subject = Some(subject);

        let wit_ref =
            Reference::with_tag(self.endpoint.registry.clone(), repo, wit_tag.to_string());
        let resp = self
            .client
            .push(&wit_ref, &layers, config, &self.auth, Some(manifest))
            .await
            .context("failed to push WIT referrer")?;

        Ok(PushWitResult {
            subject_digest,
            manifest_url: resp.manifest_url,
            config_url: resp.config_url,
        })
    }

    /// Pull WIT text via the OCI Referrers API.
    ///
    /// This is useful as a fallback when `/wasm/v1/.../wit/...` is unavailable.
    pub async fn pull_wit_via_referrers(
        &self,
        repo: &str,
        reference: &str,
        artifact_type: Option<&str>,
    ) -> Result<String> {
        let (_manifest, subject_digest) = self.pull_image_manifest(repo, reference).await?;

        // Referrers API requires the subject to be referenced by digest.
        let subject_ref = self.reference(repo, &subject_digest)?;
        let artifact_type = artifact_type.unwrap_or(WIT_ARTIFACT_TYPE_V1);
        let index = self
            .client
            .pull_referrers(&subject_ref, Some(artifact_type))
            .await
            .context("failed to pull referrers")?;

        if index.manifests.is_empty() {
            bail!("no referrers found for subject {subject_digest}");
        }

        // Deterministic selection: pick the lowest digest.
        let mut digests: Vec<String> = index.manifests.iter().map(|m| m.digest.clone()).collect();
        digests.sort();
        let referrer_digest = digests[0].clone();

        let referrer_ref = self.reference(repo, &referrer_digest)?;
        let (referrer_manifest, _referrer_digest) = self
            .client
            .pull_image_manifest(&referrer_ref, &self.auth)
            .await
            .context("failed to pull referrer manifest")?;

        let layer = referrer_manifest
            .layers
            .iter()
            .find(|l| l.media_type == WIT_LAYER_MEDIA_TYPE_V1)
            .or_else(|| referrer_manifest.layers.first())
            .context("referrer manifest has no layers")?;

        let bytes = pull_blob_to_bytes(&self.client, &referrer_ref, layer).await?;
        String::from_utf8(bytes).context("WIT blob is not valid UTF-8")
    }
}

#[derive(Debug, Clone)]
pub struct PushWitResult {
    pub subject_digest: String,
    pub manifest_url: String,
    pub config_url: String,
}

async fn pull_blob_to_bytes(
    client: &Client,
    image: &Reference,
    layer: &oci_client::manifest::OciDescriptor,
) -> Result<Vec<u8>> {
    let mut stream = client
        .pull_blob_stream(image, layer)
        .await
        .context("failed to pull blob stream")?;

    let mut out = Vec::with_capacity(stream.content_length.unwrap_or(0) as usize);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("blob stream error")?;
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}
