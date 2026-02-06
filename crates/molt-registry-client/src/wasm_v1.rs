use crate::media_types::WIT_ARTIFACT_TYPE_V1;
use crate::util::{RegistryEndpoint, apply_reqwest_auth, auth_from_env};
use anyhow::{Context, Result, bail};
use oci_client::secrets::RegistryAuth;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct WasmV1Client {
    endpoint: RegistryEndpoint,
    auth: RegistryAuth,
    http: reqwest::Client,
}

impl WasmV1Client {
    pub fn new(endpoint: RegistryEndpoint, auth: RegistryAuth) -> Result<Self> {
        Self::new_with_headers(endpoint, auth, HeaderMap::new())
    }

    /// Create a client with extra default headers (in addition to User-Agent).
    ///
    /// Note: `Authorization` should be provided via `auth`, not as a raw header.
    pub fn new_with_headers(
        endpoint: RegistryEndpoint,
        auth: RegistryAuth,
        extra: HeaderMap,
    ) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(concat!("molt-registry-client/", env!("CARGO_PKG_VERSION")))
                .context("invalid user-agent header")?,
        );
        headers.extend(extra);

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            endpoint,
            auth,
            http,
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

    fn url(&self, path: &str) -> String {
        let base = self.endpoint.base_url.as_str().trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{base}/{path}")
    }

    pub async fn wit_text(
        &self,
        repo: &str,
        reference: &str,
        opts: &WitRequest,
    ) -> Result<WitTextResponse> {
        let url = self.url(&format!(
            "/wasm/v1/{}/wit/{}",
            repo.trim_matches('/'),
            reference.trim_matches('/')
        ));

        let artifact_type = opts
            .artifact_type
            .as_deref()
            .unwrap_or(WIT_ARTIFACT_TYPE_V1);

        let mut req = self.http.get(url);
        req = apply_reqwest_auth(req, &self.auth);
        req = req.query(&[("artifactType", artifact_type)]);
        if let Some(package) = opts.package.as_deref() {
            req = req.query(&[("package", package)]);
        }

        let resp = req.send().await.context("failed to call registry")?;
        let status = resp.status();

        let etag = resp
            .headers()
            .get("ETag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let subject = resp
            .headers()
            .get("OCI-Subject")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let referrer_digest = resp
            .headers()
            .get("WIT-Referrer-Digest")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let body = resp.text().await.unwrap_or_default();

        match status.as_u16() {
            200 => Ok(WitTextResponse {
                text: body,
                etag,
                subject_digest: subject,
                referrer_manifest_digest: referrer_digest,
            }),
            202 => bail!("WIT not available yet (202 Accepted). retry later.\n{body}"),
            404 => bail!("WIT not found (404).\n{body}"),
            409 => bail!("WIT referrer is ambiguous (409).\n{body}"),
            code => bail!("registry returned HTTP {code}.\n{body}"),
        }
    }

    pub async fn interfaces(&self, repo: &str, reference: &str) -> Result<InterfacesResponse> {
        let url = self.url(&format!(
            "/wasm/v1/{}/interfaces/{}",
            repo.trim_matches('/'),
            reference.trim_matches('/')
        ));
        let req = apply_reqwest_auth(self.http.get(url), &self.auth);
        let resp = req.send().await.context("failed to call registry")?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("registry returned HTTP {}.\n{body}", status.as_u16());
        }
        serde_json::from_str(&body).context("failed to parse interfaces JSON")
    }

    pub async fn dependencies(&self, repo: &str, reference: &str) -> Result<InterfacesResponse> {
        let url = self.url(&format!(
            "/wasm/v1/{}/dependencies/{}",
            repo.trim_matches('/'),
            reference.trim_matches('/')
        ));
        let req = apply_reqwest_auth(self.http.get(url), &self.auth);
        let resp = req.send().await.context("failed to call registry")?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("registry returned HTTP {}.\n{body}", status.as_u16());
        }
        serde_json::from_str(&body).context("failed to parse dependencies JSON")
    }

    pub async fn search(&self, q: &SearchQuery) -> Result<SearchResponse> {
        let url = self.url("/wasm/v1/search");
        let mut req = apply_reqwest_auth(self.http.get(url), &self.auth);

        for v in &q.exports {
            req = req.query(&[("export", v)]);
        }
        for v in &q.imports {
            req = req.query(&[("import", v)]);
        }
        if let Some(os) = q.os.as_deref() {
            req = req.query(&[("os", os)]);
        }
        if let Some(limit) = q.limit {
            req = req.query(&[("limit", &limit.to_string())]);
        }
        if let Some(cursor) = q.cursor.as_deref() {
            req = req.query(&[("cursor", cursor)]);
        }

        let resp = req.send().await.context("failed to call registry")?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("registry returned HTTP {}.\n{body}", status.as_u16());
        }
        serde_json::from_str(&body).context("failed to parse search JSON")
    }
}

#[derive(Debug, Clone, Default)]
pub struct WitRequest {
    pub artifact_type: Option<String>,
    pub package: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WitTextResponse {
    pub text: String,
    pub etag: Option<String>,
    pub subject_digest: Option<String>,
    pub referrer_manifest_digest: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferrerDescriptor {
    pub digest: String,
    #[serde(default)]
    pub artifact_type: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfacesResponse {
    pub repo: String,
    pub reference: String,
    pub digest: String,
    pub os: String,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub subject_digest: Option<String>,
    #[serde(default)]
    pub referrers: Vec<ReferrerDescriptor>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub exports: Vec<String>,
    pub imports: Vec<String>,
    pub os: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub repo: String,
    pub digest: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub os: String,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}
