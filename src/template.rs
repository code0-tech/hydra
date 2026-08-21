use std::{fs, path::Path};

use serde_json::{Map, Value};
use tera::{Context, Tera};

use crate::{
    bundle::{BundleSource, TemplateMapping},
    env_file, ui,
};

pub fn render_setup_templates(
    source: &BundleSource,
    mappings: &[TemplateMapping],
    context: &Map<String, Value>,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let output_dir = output_dir.as_ref();

    for mapping in mappings {
        let raw = match source.template(&mapping.template) {
            Ok(raw) => raw,
            Err(error) if !mapping.required => {
                ui::warn_line(&format!(
                    "Skipping optional '{}': {error}",
                    mapping.template
                ));
                continue;
            }
            Err(error) => return Err(error),
        };
        // `.env` is reticulum's own vendored file, not a Tera template - it's
        // patched in place (same KEY=value replace `env_file::apply_values`
        // uses elsewhere) so every setting reticulum already has a sane
        // default for doesn't need duplicating into `manifest.json`; only
        // what the wizard actually collects (steps + generated secrets)
        // overrides a line.
        let rendered = if mapping.template == ".env" {
            patch_env(&raw, context)
        } else {
            render(&raw, context)?
        };
        fs::write(output_dir.join(&mapping.output), rendered)?;
    }

    Ok(())
}

fn render(raw: &str, context: &Map<String, Value>) -> anyhow::Result<String> {
    let context = Context::from_serialize(context)?;
    Ok(Tera::one_off(raw, &context, false)?)
}

fn patch_env(base: &str, context: &Map<String, Value>) -> String {
    let updates: Vec<(String, String)> = context
        .iter()
        .map(|(key, value)| (key.to_uppercase(), env_value_to_string(value)))
        .collect();
    let updates: Vec<(&str, &str)> = updates
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();

    env_file::apply_values(base, &updates)
}

/// Same textual form Tera's `{{ value }}` interpolation would have produced:
/// strings unquoted, everything else via its own `Display`.
fn env_value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
