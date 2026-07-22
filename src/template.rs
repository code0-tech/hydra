use std::{fs, path::Path};

use serde_json::{Map, Value};
use tera::{Context, Tera};

use crate::bundle::TemplateMapping;

pub fn render_setup_templates(
    bundle_dir: impl AsRef<Path>,
    mappings: &[TemplateMapping],
    context: &Map<String, Value>,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let bundle_dir = bundle_dir.as_ref();
    let output_dir = output_dir.as_ref();

    for mapping in mappings {
        render_to_file(
            bundle_dir.join(&mapping.template),
            output_dir.join(&mapping.output),
            context,
        )?;
    }

    Ok(())
}

fn render_to_file(
    template_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    context: &Map<String, Value>,
) -> anyhow::Result<()> {
    let raw = fs::read_to_string(template_path)?;
    let context = Context::from_serialize(context)?;
    let rendered = Tera::one_off(&raw, &context, false)?;

    fs::write(output_path, rendered)?;

    Ok(())
}

#[cfg(test)]
pub fn render_template(
    template_path: impl AsRef<Path>,
    context: &Map<String, Value>,
) -> anyhow::Result<()> {
    let raw = fs::read_to_string(template_path)?;
    let context = Context::from_serialize(context)?;
    Tera::one_off(&raw, &context, false)?;

    Ok(())
}
