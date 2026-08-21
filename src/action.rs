use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SERVICE_CONFIGURATION_PATH: &str = ".codezero/service.configuration.json";
pub const ACTIONS_DIR: &str = ".codezero/actions";

/// The centaurus repo/branch the action catalog is fetched from: one
/// `actions/<name>/manifest.json` per action, rather than a single
/// hand-maintained index file, so adding or updating an action doesn't
/// require a hydra release.
const CATALOG_REPO: &str = "code0-tech/centaurus";
const CATALOG_BRANCH: &str = "main";

/// Env var override for `CATALOG_BRANCH` - lets you test against an unmerged
/// centaurus branch without a code change, same as `CODEZERO_RETICULUM_BRANCH`
/// for the setup bundle: `CODEZERO_CATALOG_BRANCH=feat/new-action codezero plugin ls`.
const CATALOG_BRANCH_ENV_VAR: &str = "CODEZERO_CATALOG_BRANCH";

fn catalog_branch() -> String {
    std::env::var(CATALOG_BRANCH_ENV_VAR).unwrap_or_else(|_| CATALOG_BRANCH.to_string())
}

pub fn action_fragment_path(identifier: &str) -> PathBuf {
    Path::new(ACTIONS_DIR).join(format!("{identifier}.yml"))
}

/// Derives the `AQUILA_ACTION_<KEY>_IDENTIFIER`/`AQUILA_ACTION_<KEY>_TOKEN`
/// env var prefix an action's identifier maps to. `config-generator`'s
/// `aquila.service.configuration.json.erb` template builds aquila's actual
/// accepted-token list purely by scanning `.env` for pairs matching
/// `AQUILA_ACTION_(\S+?)_.+` - it has no access to
/// `.codezero/service.configuration.json` at all, so that's the only place
/// an installed action's token can actually take effect. Matches the
/// convention the two built-in actions already use in the bundle's `.env`
/// (`rest-action` -> `AQUILA_ACTION_REST_*`, `cron-action` -> `AQUILA_ACTION_CRON_*`).
pub fn aquila_action_env_prefix(identifier: &str) -> String {
    let key = identifier.strip_suffix("-action").unwrap_or(identifier);
    format!("AQUILA_ACTION_{}", key.to_uppercase().replace('-', "_"))
}

/// URL-encoded GitLab project path for the monorepo that builds every action
/// image (each successful pipeline on `main` tags all rebuilt images with the
/// same `RETICULUM_CONTAINER_VERSION`).
const RETICULUM_PROJECT: &str = "code0-tech%2Fdevelopment%2Freticulum";

#[derive(Debug, Deserialize)]
struct GitlabPipeline {
    id: u64,
    sha: String,
    #[serde(rename = "ref")]
    git_ref: String,
}

/// Pipelines that actually build+push images run under this ref prefix, not
/// literally `main`: reticulum's own `main`-triggered pipelines are all
/// `skipped` (its `.gitlab-ci.yml` `workflow: rules` only allow
/// `api`/`pipeline`/`web`-sourced runs, so a plain push pipeline never runs).
/// The real builds come from the GitHub Actions mirror
/// (`code0-tech/reticulum`'s `ci.yml`) triggering GitLab's API via
/// `Taucher2003/GitLab-Pipeline-Action`, which creates a synthetic
/// `glpa/main/<sha>` ref per run rather than reusing `main`.
const RETICULUM_DEV_REF_PREFIX: &str = "glpa/main/";

/// Resolves the tag of the latest successful `main` build by asking GitLab's
/// pipeline API directly, reconstructing the same
/// `0.0.0-experimental-<pipeline id>-<commit sha>` scheme reticulum's CI uses
/// for `RETICULUM_CONTAINER_VERSION`. Used only for `--dev` installs — regular
/// installs stay fully local and never make a network call.
pub fn resolve_dev_tag() -> anyhow::Result<String> {
    // Can't filter server-side by ref prefix, and `main`-triggered and
    // per-branch pipelines interleave in id order, so fetch a page and filter
    // client-side for the first that's actually a `glpa/main/*` build.
    let url = format!(
        "https://gitlab.com/api/v4/projects/{RETICULUM_PROJECT}/pipelines?status=success&order_by=id&sort=desc&per_page=20"
    );

    let pipelines: Vec<GitlabPipeline> = ureq::get(&url)
        .call()
        .map_err(|error| anyhow::anyhow!("Couldn't reach GitLab to resolve the dev tag: {error}"))?
        .into_json()
        .map_err(|error| anyhow::anyhow!("Couldn't parse GitLab's pipeline response: {error}"))?;

    let pipeline = pipelines
        .into_iter()
        .find(|pipeline| pipeline.git_ref.starts_with(RETICULUM_DEV_REF_PREFIX))
        .ok_or_else(|| {
            anyhow::anyhow!("No successful pipeline found on reticulum's main branch")
        })?;

    Ok(format!(
        "0.0.0-experimental-{}-{}",
        pipeline.id, pipeline.sha
    ))
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
    #[serde(default)]
    pub description: String,
    pub deployment: Deployment,
    /// Other actions (by identifier) this one needs installed/enabled
    /// before it's actually usable - see
    /// `command::install::check_dependencies`.
    #[serde(default)]
    pub dependencies: Vec<String>,
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

#[derive(Debug, Deserialize)]
struct GitTree {
    tree: Vec<GitTreeEntry>,
}

#[derive(Debug, Deserialize)]
struct GitTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

/// Fetches every `actions/<name>/manifest.json` on `CATALOG_BRANCH` and
/// assembles them into a catalog. One API call lists the tree, then each
/// manifest is fetched from raw.githubusercontent.com (no auth, no API rate
/// limit) rather than through the contents API.
fn fetch_remote_catalog() -> anyhow::Result<ActionCatalog> {
    let branch = catalog_branch();
    let tree_url =
        format!("https://api.github.com/repos/{CATALOG_REPO}/git/trees/{branch}?recursive=1");

    let tree: GitTree = ureq::get(&tree_url)
        .set("User-Agent", "codezero-cli")
        .call()
        .map_err(|error| anyhow::anyhow!("Couldn't reach the action catalog: {error}"))?
        .into_json()
        .map_err(|error| anyhow::anyhow!("Couldn't parse the action catalog listing: {error}"))?;

    let manifest_paths: Vec<&str> = tree
        .tree
        .iter()
        .filter(|entry| {
            entry.kind == "blob"
                && entry.path.starts_with("actions/")
                && entry.path.ends_with("/manifest.json")
        })
        .map(|entry| entry.path.as_str())
        .collect();

    let mut actions = Vec::with_capacity(manifest_paths.len());
    for path in manifest_paths {
        let raw_url = format!("https://raw.githubusercontent.com/{CATALOG_REPO}/{branch}/{path}");
        let entry: ActionEntry = ureq::get(&raw_url)
            .call()
            .map_err(|error| anyhow::anyhow!("Couldn't fetch {path}: {error}"))?
            .into_json()
            .map_err(|error| anyhow::anyhow!("Couldn't parse {path}: {error}"))?;
        actions.push(entry);
    }

    Ok(ActionCatalog { actions })
}

impl ActionCatalog {
    /// Reads the catalog from `path` when given (e.g. a local manifest.json
    /// for testing), or fetches the live catalog from centaurus otherwise.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        match path {
            Some(path) => {
                let data = fs::read_to_string(path).map_err(|error| {
                    anyhow::anyhow!(
                        "Couldn't read action catalog at {}: {error}",
                        path.display()
                    )
                })?;
                serde_json::from_str(&data).map_err(|error| {
                    anyhow::anyhow!(
                        "Couldn't parse action catalog at {}: {error}",
                        path.display()
                    )
                })
            }
            None => fetch_remote_catalog(),
        }
    }

    /// Matches by `name`, `identifier`, or the identifier with a trailing
    /// `-action` dropped - so `gls-action` is installable as `gls` without
    /// every manifest having to redundantly declare a short `name` too.
    pub fn find(&self, name: &str) -> Option<&ActionEntry> {
        self.actions.iter().find(|action| {
            action.name == name
                || action.identifier == name
                || action.identifier.strip_suffix("-action") == Some(name)
        })
    }

    pub fn names(&self) -> Vec<&str> {
        self.actions
            .iter()
            .map(|action| action.name.as_str())
            .collect()
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
            anyhow::anyhow!(
                "Couldn't read service configuration at {}: {error}",
                path.display()
            )
        })?;

        let config: ServiceConfiguration = serde_json::from_str(&data).map_err(|error| {
            anyhow::anyhow!(
                "Couldn't parse service configuration at {}: {error}",
                path.display()
            )
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
        self.actions
            .retain(|action| action.identifier != identifier);
        self.actions.len() != before
    }
}

/// Re-merges `codezero plugin install`/`uninstall`-recorded actions into
/// the service configuration file after a fresh template render. Whatever
/// the bundle's `service.configuration.json.tera` declares (e.g. reticulum's
/// built-in `rest-action`/`cron-action`) is reticulum's call, not hydra's -
/// this only adds back what the CLI itself is responsible for, via
/// `upsert_action` (merge), never wholesale replacing `.actions` and losing
/// what the template just rendered.
pub fn carry_forward_actions_at(
    path: impl AsRef<Path>,
    previously_installed: Vec<ActionConfig>,
) -> anyhow::Result<()> {
    let mut config = ServiceConfiguration::load_or_default(&path)?;

    for action in previously_installed {
        config.upsert_action(&action.identifier, &action.token);
    }

    config.save(&path)
}

pub fn carry_forward_actions(previously_installed: Vec<ActionConfig>) -> anyhow::Result<()> {
    carry_forward_actions_at(SERVICE_CONFIGURATION_PATH, previously_installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_a_catalog_from_disk() {
        let path = std::env::temp_dir().join(format!(
            "hydra-action-catalog-test-{}.json",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"{
                "actions": [
                    {
                        "name": "gls",
                        "identifier": "gls-action",
                        "description": "GLS shipping integration",
                        "deployment": { "docker": { "image": "ghcr.io/code0-tech/centaurus/ci-builds/gls-action" } },
                        "dependencies": ["rest-action"]
                    }
                ]
            }"#,
        )
        .unwrap();

        let catalog = ActionCatalog::load(Some(&path)).expect("catalog should load");

        assert_eq!(catalog.actions.len(), 1);
        let gls = catalog.find("gls").expect("gls should be findable by name");
        assert_eq!(gls.identifier, "gls-action");
        assert_eq!(gls.deployment.docker.aquila_url_var, "AQUILA_URL");
        assert_eq!(gls.deployment.docker.auth_token_var, "AUTH_TOKEN");
        assert_eq!(gls.dependencies, vec!["rest-action"]);

        let by_identifier = catalog
            .find("gls-action")
            .expect("gls should also be findable by identifier");
        assert_eq!(by_identifier.name, "gls");

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn finds_an_action_by_identifier_with_the_action_suffix_stripped() {
        let path = std::env::temp_dir().join(format!(
            "hydra-action-catalog-test-{}-suffix.json",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"{
                "actions": [
                    {
                        "name": "shopify-action",
                        "identifier": "shopify-action",
                        "deployment": { "docker": { "image": "ghcr.io/code0-tech/centaurus/ci-builds/shopify-action" } }
                    }
                ]
            }"#,
        )
        .unwrap();

        let catalog = ActionCatalog::load(Some(&path)).expect("catalog should load");
        let entry = catalog
            .find("shopify")
            .expect("shopify should resolve via the stripped identifier");
        assert_eq!(entry.identifier, "shopify-action");

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn unknown_action_is_not_found() {
        let path = std::env::temp_dir().join(format!(
            "hydra-action-catalog-test-{}-empty.json",
            std::process::id()
        ));
        fs::write(&path, r#"{"actions": []}"#).unwrap();

        let catalog = ActionCatalog::load(Some(&path)).expect("catalog should load");
        assert!(catalog.find("nonexistent").is_none());

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn dependencies_default_to_empty_when_omitted() {
        let entry: ActionEntry = serde_json::from_str(
            r#"{
                "name": "rest-action",
                "identifier": "rest-action",
                "description": "Webhook triggered by an incoming HTTP request.",
                "deployment": { "docker": { "image": "ghcr.io/code0-tech/centaurus/ci-builds/rest-action" } }
            }"#,
        )
        .expect("manifest should parse");

        assert!(entry.dependencies.is_empty());
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

    #[test]
    fn aquila_action_env_prefix_strips_the_action_suffix_and_uppercases() {
        assert_eq!(aquila_action_env_prefix("gls-action"), "AQUILA_ACTION_GLS");
        assert_eq!(
            aquila_action_env_prefix("rest-action"),
            "AQUILA_ACTION_REST"
        );
    }

    #[test]
    fn aquila_action_env_prefix_replaces_remaining_dashes() {
        assert_eq!(
            aquila_action_env_prefix("multi-word-action"),
            "AQUILA_ACTION_MULTI_WORD"
        );
    }

    #[test]
    fn carry_forward_actions_merges_onto_whatever_the_template_just_rendered() {
        let path = std::env::temp_dir().join(format!(
            "hydra-carry-forward-test-{}.json",
            std::process::id()
        ));

        // Simulates a fresh template render: the bundle's own built-in
        // actions (e.g. rest-action/cron-action) are already present.
        let rendered = ServiceConfiguration {
            actions: vec![ActionConfig {
                identifier: "rest-action".into(),
                token: "rest-token".into(),
                configs: vec![],
            }],
            runtimes: vec![],
        };
        rendered.save(&path).unwrap();

        let previously_installed = vec![ActionConfig {
            identifier: "gls-action".into(),
            token: "gls-token".into(),
            configs: vec![],
        }];

        carry_forward_actions_at(&path, previously_installed).unwrap();

        let result = ServiceConfiguration::load_or_default(&path).unwrap();
        assert_eq!(result.actions.len(), 2);
        assert!(
            result
                .actions
                .iter()
                .any(|a| a.identifier == "rest-action" && a.token == "rest-token")
        );
        assert!(
            result
                .actions
                .iter()
                .any(|a| a.identifier == "gls-action" && a.token == "gls-token")
        );

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn carry_forward_actions_updates_an_existing_entrys_token() {
        let path = std::env::temp_dir().join(format!(
            "hydra-carry-forward-test-{}-update.json",
            std::process::id()
        ));

        let rendered = ServiceConfiguration {
            actions: vec![ActionConfig {
                identifier: "gls-action".into(),
                token: "old-token".into(),
                configs: vec![],
            }],
            runtimes: vec![],
        };
        rendered.save(&path).unwrap();

        let previously_installed = vec![ActionConfig {
            identifier: "gls-action".into(),
            token: "new-token".into(),
            configs: vec![],
        }];

        carry_forward_actions_at(&path, previously_installed).unwrap();

        let result = ServiceConfiguration::load_or_default(&path).unwrap();
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].token, "new-token");

        fs::remove_file(&path).unwrap();
    }
}
