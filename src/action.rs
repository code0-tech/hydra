use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::embedded;

pub const SERVICE_CONFIGURATION_PATH: &str = ".codezero/service.configuration.json";
pub const ACTIONS_DIR: &str = ".codezero/actions";

pub fn action_fragment_path(identifier: &str) -> PathBuf {
    Path::new(ACTIONS_DIR).join(format!("{identifier}.yml"))
}

/// URL-encoded GitLab project path for the monorepo that builds every action
/// image (each successful pipeline on `main` tags all rebuilt images with the
/// same `RETICULUM_CONTAINER_VERSION`).
const RETICULUM_PROJECT: &str = "code0-tech%2Fdevelopment%2Freticulum";

#[derive(Debug, Deserialize)]
struct GitlabPipeline {
    id: u64,
    sha: String,
}

/// Resolves the tag of the latest successful `main` build by asking GitLab's
/// pipeline API directly, reconstructing the same
/// `0.0.0-experimental-<pipeline id>-<commit sha>` scheme reticulum's CI uses
/// for `RETICULUM_CONTAINER_VERSION`. Used only for `--dev` installs — regular
/// installs stay fully local and never make a network call.
pub fn resolve_dev_tag() -> anyhow::Result<String> {
    let url = format!(
        "https://gitlab.com/api/v4/projects/{RETICULUM_PROJECT}/pipelines?ref=main&status=success&order_by=id&sort=desc&per_page=1"
    );

    let pipelines: Vec<GitlabPipeline> = ureq::get(&url)
        .call()
        .map_err(|error| anyhow::anyhow!("Couldn't reach GitLab to resolve the dev tag: {error}"))?
        .into_json()
        .map_err(|error| anyhow::anyhow!("Couldn't parse GitLab's pipeline response: {error}"))?;

    let pipeline = pipelines
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No successful pipeline found on reticulum's main branch"))?;

    Ok(format!("0.0.0-experimental-{}-{}", pipeline.id, pipeline.sha))
}

fn default_aquila_url_var() -> String {
    "AQUILA_URL".into()
}

fn default_auth_token_var() -> String {
    "AUTH_TOKEN".into()
}

#[derive(Debug, Deserialize)]
pub struct ActionCatalog {
    pub actions: Vec<ActionEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ActionEntry {
    pub name: String,
    pub identifier: String,
    pub version: String,
    pub deployment: Deployment,
}

#[derive(Debug, Deserialize)]
pub struct Deployment {
    pub docker: DockerDeployment,
}

#[derive(Debug, Deserialize)]
pub struct DockerDeployment {
    pub image: String,
    #[serde(default = "default_aquila_url_var")]
    pub aquila_url_var: String,
    #[serde(default = "default_auth_token_var")]
    pub auth_token_var: String,
}

impl ActionCatalog {
    /// Reads the catalog from `path` when given, or falls back to the copy
    /// baked into the binary - same disk-vs-embedded split as `SetupBundle`.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let data = match path {
            Some(path) => fs::read_to_string(path)
                .map_err(|error| anyhow::anyhow!("Couldn't read action catalog at {}: {error}", path.display()))?,
            None => embedded::ACTIONS_INDEX_JSON.to_string(),
        };

        let catalog: ActionCatalog =
            serde_json::from_str(&data).map_err(|error| anyhow::anyhow!("Couldn't parse action catalog: {error}"))?;

        Ok(catalog)
    }

    pub fn find(&self, name: &str) -> Option<&ActionEntry> {
        self.actions.iter().find(|action| action.name == name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.actions.iter().map(|action| action.name.as_str()).collect()
    }
}

/// Splits an install spec like "gls@1.2.3" into ("gls", Some("1.2.3")),
/// or "gls" into ("gls", None).
pub fn parse_install_spec(spec: &str) -> (&str, Option<&str>) {
    match spec.split_once('@') {
        Some((name, version)) => (name, Some(version)),
        None => (spec, None),
    }
}

/// Aquila's on-disk service configuration file, matching
/// `SerializableServiceConfiguration` in aquila/src/configuration/service.rs.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ServiceConfiguration {
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
    #[serde(default)]
    pub runtimes: Vec<RuntimeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfig {
    pub token: String,
    pub identifier: String,
    #[serde(default)]
    pub configs: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub token: String,
    pub identifier: String,
}

impl ServiceConfiguration {
    pub fn load_or_default(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(Self::default());
        }

        let data = fs::read_to_string(path).map_err(|error| {
            anyhow::anyhow!("Couldn't read service configuration at {}: {error}", path.display())
        })?;

        let config: ServiceConfiguration = serde_json::from_str(&data).map_err(|error| {
            anyhow::anyhow!("Couldn't parse service configuration at {}: {error}", path.display())
        })?;

        Ok(config)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Inserts or replaces the action's token by identifier. Returns true if
    /// it replaced an existing entry rather than appending a new one.
    pub fn upsert_action(&mut self, identifier: &str, token: &str) -> bool {
        match self
            .actions
            .iter_mut()
            .find(|action| action.identifier == identifier)
        {
            Some(action) => {
                action.token = token.to_string();
                true
            }
            None => {
                self.actions.push(ActionConfig {
                    token: token.to_string(),
                    identifier: identifier.to_string(),
                    configs: Vec::new(),
                });
                false
            }
        }
    }

    /// Removes the action with the given identifier. Returns true if an entry
    /// was actually removed.
    pub fn remove_action(&mut self, identifier: &str) -> bool {
        let before = self.actions.len();
        self.actions.retain(|action| action.identifier != identifier);
        self.actions.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_bundled_catalog() {
        let catalog = ActionCatalog::load(Some(Path::new("actions/index.json"))).expect("catalog should load");

        assert_eq!(catalog.actions.len(), 4);
        let gls = catalog.find("gls").expect("gls should be in the catalog");
        assert_eq!(gls.identifier, "gls-action");
        assert_eq!(gls.deployment.docker.aquila_url_var, "AQUILA_URL");
        assert_eq!(gls.deployment.docker.auth_token_var, "AUTH_TOKEN");
    }

    #[test]
    fn unknown_action_is_not_found() {
        let catalog = ActionCatalog::load(Some(Path::new("actions/index.json"))).expect("catalog should load");
        assert!(catalog.find("nonexistent").is_none());
    }

    #[test]
    fn loads_the_embedded_catalog_identically_to_disk() {
        let disk = ActionCatalog::load(Some(Path::new("actions/index.json"))).expect("disk catalog should load");
        let embedded = ActionCatalog::load(None).expect("embedded catalog should load");

        assert_eq!(disk.actions.len(), embedded.actions.len());
        assert_eq!(disk.names(), embedded.names());
    }

    #[test]
    fn parses_name_with_and_without_version() {
        assert_eq!(parse_install_spec("gls@1.2.3"), ("gls", Some("1.2.3")));
        assert_eq!(parse_install_spec("gls"), ("gls", None));
    }

    #[test]
    fn upsert_action_appends_new_entries() {
        let mut config = ServiceConfiguration::default();

        let replaced = config.upsert_action("gls-action", "token-a");

        assert!(!replaced);
        assert_eq!(config.actions.len(), 1);
        assert_eq!(config.actions[0].token, "token-a");
    }

    #[test]
    fn upsert_action_replaces_existing_entries_in_place() {
        let mut config = ServiceConfiguration::default();
        config.upsert_action("gls-action", "token-a");

        let replaced = config.upsert_action("gls-action", "token-b");

        assert!(replaced);
        assert_eq!(config.actions.len(), 1);
        assert_eq!(config.actions[0].token, "token-b");
    }

    #[test]
    fn remove_action_deletes_matching_entry_only() {
        let mut config = ServiceConfiguration::default();
        config.upsert_action("gls-action", "token-a");
        config.upsert_action("shopware-action", "token-b");

        let removed = config.remove_action("gls-action");

        assert!(removed);
        assert_eq!(config.actions.len(), 1);
        assert_eq!(config.actions[0].identifier, "shopware-action");
    }

    #[test]
    fn remove_action_is_a_no_op_when_missing() {
        let mut config = ServiceConfiguration::default();
        config.upsert_action("shopware-action", "token-b");

        let removed = config.remove_action("gls-action");

        assert!(!removed);
        assert_eq!(config.actions.len(), 1);
    }
}
