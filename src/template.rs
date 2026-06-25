use std::{fs, path::Path};

use serde::Serialize;
use tera::{Context, Tera};

#[derive(Debug, serde::Serialize)]
pub struct TemplateProps {
    pub initial_root_mail: String,
    pub initial_root_password: String,
    pub initial_runtime_token: String,
    pub compose_profiles: String,
    pub image_registry: String,
    pub image_tag: String,
    pub image_edition: String,
    pub hostname: String,
    pub http_port: u16,
    pub https_port: u16,
    pub ssl_enabled: bool,
    pub postgres_db: String,
    pub postgres_user: String,
    pub postgres_password: String,
    pub aquila_sagittarius_url: String,
    pub aquila_sagittarius_token: String,
    pub draco_rest_port: u16,
    pub draco_rest_host: String,
    pub taurus_aquila_token: String,
    pub draco_rest_aquila_token: String,
    pub draco_cron_aquila_token: String,
    pub velorum_host: String,
    pub velorum_port: u16,
    pub velorum_jwt_secret: String,
    pub sagittarius_rails_host: String,
    pub sagittarius_rails_port: u16,
    pub sagittarius_cable_host: String,
    pub sagittarius_cable_port: u16,
    pub sagittarius_grpc_host: String,
    pub sagittarius_grpc_port: u16,
    pub sagittarius_log_level: String,
    pub sculptor_host: String,
    pub sculptor_port: u16,
    pub postgres_host: String,
    pub postgres_port: u16,
}

pub fn render_setup_templates(
    output_dir: impl AsRef<Path>,
    props: &TemplateProps,
) -> anyhow::Result<()> {
    let output_dir = output_dir.as_ref();

    render_to_file("templates/env.tmpl", output_dir.join(".env"), props)?;
    render_to_file(
        "templates/docker-compose.yml.tmpl",
        output_dir.join("docker-compose.yml"),
        props,
    )?;

    Ok(())
}

fn render_to_file<T: Serialize>(
    template_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    props: &T,
) -> anyhow::Result<()> {
    let raw = fs::read_to_string(template_path)?;
    let context = Context::from_serialize(props)?;
    let rendered = Tera::one_off(&raw, &context, false)?;

    fs::write(output_path, rendered)?;

    Ok(())
}

#[cfg(test)]
pub fn render_template(
    template_path: impl AsRef<Path>,
    props: &TemplateProps,
) -> anyhow::Result<()> {
    let raw = fs::read_to_string(template_path)?;
    let context = Context::from_serialize(props)?;
    Tera::one_off(&raw, &context, false)?;

    Ok(())
}
