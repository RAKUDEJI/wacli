use anyhow::{Context, Result, bail};
use base64::Engine;
use oci_client::secrets::RegistryAuth;
use reqwest::RequestBuilder;
use url::Url;

#[derive(Debug, Clone)]
pub struct RegistryEndpoint {
    /// Base URL with scheme, e.g. `https://registry.example.com`.
    pub base_url: Url,
    /// Registry host (and optional port) used by OCI references.
    pub registry: String,
}

impl RegistryEndpoint {
    pub fn parse(base_url: &str) -> Result<Self> {
        let url = Url::parse(base_url)
            .with_context(|| format!("invalid registry URL (expected https://...): {base_url}"))?;

        let host = url
            .host_str()
            .context("registry URL missing host")?
            .to_string();
        let registry = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host,
        };

        // `oci-client` always targets `/v2/...` at the origin. If we ever need a
        // subpath-mounted registry, we need a different strategy.
        if url.path() != "/" && !url.path().is_empty() {
            bail!(
                "registry URL must not contain a path (got '{}')",
                url.path()
            );
        }

        Ok(Self {
            base_url: strip_trailing_slash(url)?,
            registry,
        })
    }
}

fn strip_trailing_slash(url: Url) -> Result<Url> {
    let s = url.as_str();
    if s.ends_with('/') {
        Url::parse(s.trim_end_matches('/')).context("failed to normalize registry URL")
    } else {
        Ok(url)
    }
}

pub fn auth_from_env() -> Result<RegistryAuth> {
    if let Ok(v) = std::env::var("MOLT_AUTH_HEADER") {
        let v = v.trim();
        if !v.is_empty() {
            return auth_from_header_line(v);
        }
    }

    let username = std::env::var("MOLT_USERNAME")
        .ok()
        .or_else(|| std::env::var("USERNAME").ok());
    let password = std::env::var("MOLT_PASSWORD")
        .ok()
        .or_else(|| std::env::var("PASSWORD").ok());

    match (username, password) {
        (Some(u), Some(p)) => {
            let u = u.trim().to_string();
            let p = p.trim().to_string();
            if u.is_empty() || p.is_empty() {
                bail!("registry username/password must not be empty");
            }
            Ok(RegistryAuth::Basic(u, p))
        }
        (None, None) => Ok(RegistryAuth::Anonymous),
        _ => bail!(
            "registry username/password must both be set (USERNAME/PASSWORD or MOLT_USERNAME/MOLT_PASSWORD)"
        ),
    }
}

pub fn auth_from_header_line(line: &str) -> Result<RegistryAuth> {
    let (k, v) = line.split_once(':').with_context(|| {
        format!("invalid MOLT_AUTH_HEADER (expected 'Authorization: ...'): {line}")
    })?;
    if k.trim().to_ascii_lowercase() != "authorization" {
        bail!(
            "MOLT_AUTH_HEADER must be an Authorization header (got '{}')",
            k.trim()
        );
    }

    let v = v.trim();
    if let Some(token) = v.strip_prefix("Bearer ") {
        let token = token.trim();
        if token.is_empty() {
            bail!("invalid Authorization: Bearer header (empty token)");
        }
        return Ok(RegistryAuth::Bearer(token.to_string()));
    }

    if let Some(b64) = v.strip_prefix("Basic ") {
        let b64 = b64.trim();
        if b64.is_empty() {
            bail!("invalid Authorization: Basic header (empty value)");
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .context("invalid base64 in Authorization: Basic header")?;
        let decoded = String::from_utf8(decoded)
            .context("Authorization: Basic decoded value is not valid UTF-8")?;
        let (u, p) = decoded
            .split_once(':')
            .context("Authorization: Basic must decode to 'username:password'")?;
        let u = u.trim().to_string();
        let p = p.trim().to_string();
        if u.is_empty() || p.is_empty() {
            bail!("Authorization: Basic decoded username/password must not be empty");
        }
        return Ok(RegistryAuth::Basic(u, p));
    }

    bail!("unsupported Authorization scheme (expected Bearer or Basic): {v}");
}

pub fn apply_reqwest_auth(req: RequestBuilder, auth: &RegistryAuth) -> RequestBuilder {
    match auth {
        RegistryAuth::Anonymous => req,
        RegistryAuth::Basic(u, p) => req.basic_auth(u, Some(p)),
        RegistryAuth::Bearer(t) => req.bearer_auth(t),
    }
}

/// Sanitize a string for use as a directory name segment on common filesystems.
///
/// This is used for cache paths like `.wacli/framework/<repo>/<reference>/...`.
pub fn sanitize_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            // Windows reserved characters
            ':' | '<' | '>' | '"' | '\\' | '|' | '?' | '*' => out.push('_'),
            // Keep '/' as directory separator out of segments.
            '/' => out.push('_'),
            c if c.is_control() => out.push('_'),
            c => out.push(c),
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_endpoint_parse_strips_trailing_slash() {
        let ep = RegistryEndpoint::parse("https://example.com").unwrap();
        assert_eq!(ep.base_url.as_str(), "https://example.com/");
        assert_eq!(ep.registry, "example.com");
    }

    #[test]
    fn registry_endpoint_parse_rejects_path_prefix() {
        let err = RegistryEndpoint::parse("https://example.com/registry").unwrap_err();
        assert!(err.to_string().contains("must not contain a path"));
    }

    #[test]
    fn sanitize_path_segment_replaces_bad_chars() {
        assert_eq!(sanitize_path_segment("sha256:abc/def"), "sha256_abc_def");
    }
}
