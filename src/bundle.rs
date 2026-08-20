use std::{fs, path::PathBuf};

use rand::{Rng, distributions::Alphanumeric};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::embedded;

/// Where the setup bundle (manifest + templates) is read from: a directory on
/// disk when the user passes `--bundle` explicitly, or the copy baked into the
/// binary otherwise - which is what makes an installed `codezero` work without
/// this repo checked out nearby.
pub enum BundleSource {
    Disk(PathBuf),
    Embedded,
}

impl BundleSource {
    pub fn from_arg(path: Option<PathBuf>) -> Self {
        match path {
            Some(path) => BundleSource::Disk(path),
            None => BundleSource::Embedded,
        }
    }

    fn manifest_json(&self) -> anyhow::Result<String> {
        match self {
            BundleSource::Disk(dir) => {
                let path = dir.join("manifest.json");
                fs::read_to_string(&path)
                    .map_err(|error| anyhow::anyhow!("Couldn't read setup bundle manifest at {}: {error}", path.display()))
            }
            BundleSource::Embedded => Ok(embedded::MANIFEST_JSON.to_string()),
        }
    }

    /// Reads one of the template files a manifest's `templates` list refers
    /// to by filename (e.g. `"env.tera"`).
    pub fn template(&self, name: &str) -> anyhow::Result<String> {
        match self {
            BundleSource::Disk(dir) => {
                let path = dir.join(name);
                fs::read_to_string(&path)
                    .map_err(|error| anyhow::anyhow!("Couldn't read template at {}: {error}", path.display()))
            }
            BundleSource::Embedded => embedded::template(name)
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("No embedded template named '{name}'")),
        }
    }
}

fn default_token_length() -> usize {
    64
}

/// One id, or several ids that should all receive the same generated value
/// (e.g. a shared secret referenced by both ends of a connection).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TokenIds {
    One(String),
    Many(Vec<String>),
}

impl TokenIds {
    pub fn as_slice(&self) -> Vec<&str> {
        match self {
            TokenIds::One(id) => vec![id.as_str()],
            TokenIds::Many(ids) => ids.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SetupBundle {
    #[allow(dead_code)]
    pub version: String,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub secrets: Vec<Secret>,
    #[serde(rename = "static", default)]
    pub static_values: Map<String, Value>,
    pub templates: Vec<TemplateMapping>,
}

/// A random secret generated with no user prompt and assigned to one or more
/// context keys (e.g. a shared token referenced by both ends of a connection).
#[derive(Debug, Deserialize)]
pub struct Secret {
    pub id: TokenIds,
    #[serde(default = "default_token_length")]
    pub length: usize,
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TemplateMapping {
    pub template: String,
    pub output: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Choice {
    pub label: String,
    pub value: Value,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    Input {
        id: String,
        section: String,
        prompt: String,
        default: Option<String>,
    },
    Password {
        id: String,
        section: String,
        prompt: String,
        #[serde(default)]
        confirm: bool,
        /// Only used to auto-fill this step in `--dev` mode; dialoguer's
        /// password prompt has no notion of a displayed default, so this
        /// never changes the interactive prompt itself.
        default: Option<String>,
    },
    Select {
        id: String,
        section: String,
        prompt: String,
        options: Vec<Choice>,
        #[serde(default)]
        allow_custom: bool,
        custom_prompt: Option<String>,
    },
    Multiselect {
        id: String,
        section: String,
        prompt: String,
        options: Vec<Choice>,
        #[serde(default)]
        join: Option<String>,
    },
}

impl Step {
    pub fn section(&self) -> &str {
        match self {
            Step::Input { section, .. }
            | Step::Password { section, .. }
            | Step::Select { section, .. }
            | Step::Multiselect { section, .. } => section,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Step::Input { id, .. }
            | Step::Password { id, .. }
            | Step::Select { id, .. }
            | Step::Multiselect { id, .. } => id,
        }
    }
}

/// Picks the option flagged as the default, falling back to the first one.
/// Used both for interactive `Select` defaults and to auto-resolve a select
/// step's value when a prompt is skipped entirely (e.g. `--dev`).
pub fn default_choice(options: &[Choice]) -> Option<&Choice> {
    options.iter().find(|choice| choice.default).or_else(|| options.first())
}

/// A random alphanumeric string suitable for use as a shared secret/token.
pub fn generate_token(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

impl SetupBundle {
    pub fn load(source: &BundleSource) -> anyhow::Result<Self> {
        let data = source.manifest_json()?;

        let bundle: SetupBundle = serde_json::from_str(&data)
            .map_err(|error| anyhow::anyhow!("Couldn't parse setup bundle manifest: {error}"))?;

        Ok(bundle)
    }
}

/// Joins selected multiselect choice values into a single comma-separated
/// string (or the configured separator), or returns them as a JSON array
/// when no separator is configured.
pub fn join_multiselect(selected: &[Value], join: &Option<String>) -> Value {
    match join {
        Some(separator) => {
            let joined = selected
                .iter()
                .map(|value| match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(separator);

            Value::String(joined)
        }
        None => Value::Array(selected.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_bundled_manifest() {
        let bundle = SetupBundle::load(&BundleSource::Disk(PathBuf::from("bundle"))).expect("bundle should load");

        assert!(!bundle.steps.is_empty());
        assert!(!bundle.secrets.is_empty());
        assert_eq!(bundle.templates.len(), 3);
        assert_eq!(
            bundle.static_values.get("hostname"),
            Some(&Value::String("localhost".into()))
        );
    }

    #[test]
    fn loads_the_embedded_manifest_identically_to_disk() {
        let disk = SetupBundle::load(&BundleSource::Disk(PathBuf::from("bundle"))).expect("disk bundle should load");
        let embedded = SetupBundle::load(&BundleSource::Embedded).expect("embedded bundle should load");

        assert_eq!(disk.steps.len(), embedded.steps.len());
        assert_eq!(disk.secrets.len(), embedded.secrets.len());
        assert_eq!(disk.templates.len(), embedded.templates.len());
    }

    #[test]
    fn embedded_template_matches_disk_for_every_mapping() {
        let bundle = SetupBundle::load(&BundleSource::Disk(PathBuf::from("bundle"))).expect("bundle should load");
        let disk = BundleSource::Disk(PathBuf::from("bundle"));

        for mapping in &bundle.templates {
            let from_disk = disk.template(&mapping.template).expect("disk template should read");
            let from_embedded = BundleSource::Embedded
                .template(&mapping.template)
                .unwrap_or_else(|_| panic!("'{}' should be embedded", mapping.template));
            assert_eq!(from_disk, from_embedded, "{} differs between disk and embedded", mapping.template);
        }
    }

    #[test]
    fn joins_selected_values_with_separator() {
        let selected = vec![Value::String("ide".into()), Value::String("runtime".into())];
        let joined = join_multiselect(&selected, &Some(",".into()));

        assert_eq!(joined, Value::String("ide,runtime".into()));
    }

    #[test]
    fn keeps_selected_values_as_array_without_separator() {
        let selected = vec![Value::String("ide".into())];
        let joined = join_multiselect(&selected, &None);

        assert_eq!(joined, Value::Array(vec![Value::String("ide".into())]));
    }

    #[test]
    fn default_choice_prefers_the_flagged_option() {
        let options = vec![
            Choice { label: "a".into(), value: Value::String("a".into()), default: false },
            Choice { label: "b".into(), value: Value::String("b".into()), default: true },
        ];

        assert_eq!(default_choice(&options).unwrap().label, "b");
    }

    #[test]
    fn default_choice_falls_back_to_first_option() {
        let options = vec![
            Choice { label: "a".into(), value: Value::String("a".into()), default: false },
            Choice { label: "b".into(), value: Value::String("b".into()), default: false },
        ];

        assert_eq!(default_choice(&options).unwrap().label, "a");
    }

    #[test]
    fn generates_tokens_of_the_requested_length_and_uniqueness() {
        let a = generate_token(64);
        let b = generate_token(64);

        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, b);
    }
}
