use std::{fs, path::Path};

use serde_json::{Map, Value};
use tera::{Context, Tera};

use crate::bundle::{BundleSource, TemplateMapping};

pub fn render_setup_templates(
    source: &BundleSource,
    mappings: &[TemplateMapping],
    context: &Map<String, Value>,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let output_dir = output_dir.as_ref();

    for mapping in mappings {
        let raw = source.template(&mapping.template)?;
        let rendered = render(&raw, context)?;
        fs::write(output_dir.join(&mapping.output), rendered)?;
    }

    Ok(())
}

fn render(raw: &str, context: &Map<String, Value>) -> anyhow::Result<String> {
    let context = Context::from_serialize(context)?;
    Ok(Tera::one_off(raw, &context, false)?)
}

#[cfg(test)]
pub fn render_template(raw: &str, context: &Map<String, Value>) -> anyhow::Result<()> {
    render(raw, context)?;
    Ok(())
}
