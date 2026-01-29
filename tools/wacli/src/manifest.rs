use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub package: Package,
    pub framework: Framework,
    #[serde(default)]
    pub command: Vec<Command>,
    #[serde(default)]
    pub output: Option<Output>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Package {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Framework {
    pub host: PathBuf,
    pub core: PathBuf,
    pub registry: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Command {
    /// CLI command name (used for dispatch)
    pub name: String,
    /// Component package name (e.g., "example:greeter")
    #[serde(default)]
    pub package: Option<String>,
    /// Path to the plugin component
    pub plugin: PathBuf,
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl Command {
    /// Get the package name, defaulting to "example:{name}" if not specified
    pub fn package_name(&self) -> String {
        self.package
            .clone()
            .unwrap_or_else(|| format!("example:{}", self.name))
    }

    /// Get the variable name for WAC (replacing - with _)
    /// Derived from package name: "example:greeter" -> "greeter"
    pub fn var_name(&self) -> String {
        let pkg = self.package_name();
        // Extract the last part after ":"
        pkg.rsplit(':')
            .next()
            .unwrap_or(&self.name)
            .replace('-', "_")
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Output {
    pub path: PathBuf,
}

impl Manifest {
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest: {}", path.display()))?;
        Self::from_str(&contents)
    }

    pub fn from_str(contents: &str) -> Result<Self> {
        toml::from_str(contents).context("failed to parse manifest")
    }

    pub fn output_path(&self) -> PathBuf {
        self.output
            .as_ref()
            .map(|o| o.path.clone())
            .unwrap_or_else(|| PathBuf::from("dist/output.component.wasm"))
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            package: Package {
                name: "example:my-cli".to_string(),
                version: Some("0.1.0".to_string()),
            },
            framework: Framework {
                host: PathBuf::from("components/host/host.component.wasm"),
                core: PathBuf::from("components/core/core.component.wasm"),
                registry: PathBuf::from("registry/registry.component.wasm"),
            },
            command: vec![Command {
                name: "hello".to_string(),
                package: None,
                plugin: PathBuf::from("plugins/hello/hello.component.wasm"),
                aliases: vec![],
            }],
            output: Some(Output {
                path: PathBuf::from("dist/my-cli.component.wasm"),
            }),
        }
    }
}
