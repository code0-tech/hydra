use std::fs;

use console::{Style, style};
use dialoguer::{Confirm, Input, MultiSelect, Password, Select, theme::ColorfulTheme};

use crate::{
    runner::{Runner, default_runner},
    settings::{profile::Profile, registry::Registry},
    template::{TemplateProps, render_setup_templates},
};

const BANNER: &str = include_str!("../../assets/banner.txt");

#[derive(Debug)]
struct SetupProps {
    initial_username: String,
    initial_password: String,
    profiles: Vec<Profile>,
    registry: String,
    version: String,
}

fn print_banner() {
    println!();
    for line in BANNER.lines() {
        println!("{}", style(line).cyan());
    }
    println!();
    println!("{}", style("Interactive setup").magenta().bold());
    println!(
        "{}",
        style("Create a local .codezero runtime configuration, then start it.").dim()
    );
    println!();
}

fn print_step(index: usize, total: usize, title: &str) {
    println!();
    println!(
        "{} {}",
        style(format!("[{index}/{total}]")).cyan().bold(),
        style(title).bold()
    );
}

fn wizard_theme() -> ColorfulTheme {
    ColorfulTheme {
        defaults_style: Style::new().for_stderr().cyan(),
        prompt_style: Style::new().for_stderr().bold(),
        prompt_prefix: style(">".to_string()).for_stderr().magenta().bold(),
        prompt_suffix: style(":".to_string()).for_stderr().black().bright(),
        success_prefix: style(">".to_string()).for_stderr().green().bold(),
        success_suffix: style("".to_string()).for_stderr(),
        error_prefix: style("!".to_string()).for_stderr().red().bold(),
        error_style: Style::new().for_stderr().red(),
        hint_style: Style::new().for_stderr().black().bright(),
        values_style: Style::new().for_stderr().cyan(),
        active_item_style: Style::new().for_stderr().cyan().bold(),
        inactive_item_style: Style::new().for_stderr(),
        active_item_prefix: style(">".to_string()).for_stderr().magenta().bold(),
        inactive_item_prefix: style(" ".to_string()).for_stderr(),
        checked_item_prefix: style("[x]".to_string()).for_stderr().green(),
        unchecked_item_prefix: style("[ ]".to_string()).for_stderr().black().bright(),
        picked_item_prefix: style(">".to_string()).for_stderr().magenta().bold(),
        unpicked_item_prefix: style(" ".to_string()).for_stderr(),
    }
}

fn collect_setup_props() -> anyhow::Result<SetupProps> {
    let theme = wizard_theme();

    print_banner();
    print_step(1, 4, "Admin account");

    let username: String = Input::with_theme(&theme)
        .with_prompt("Root email")
        .default("root@code0.tech".into())
        .interact_text()?;

    let password: String = Password::with_theme(&theme)
        .with_prompt("Root password")
        .with_confirmation("Confirm root password", "Passwords do not match")
        .interact()?;

    print_step(2, 4, "Runtime profiles");

    let profiles = ["ide", "runtime", "ai"];
    let selected_profiles = MultiSelect::with_theme(&theme)
        .with_prompt("Choose profiles")
        .items(profiles)
        .defaults(&[true, true, false])
        .interact()?;

    anyhow::ensure!(
        !selected_profiles.is_empty(),
        "select at least one runtime profile"
    );

    let parsed_profiles: Vec<Profile> = selected_profiles
        .iter()
        .map(|x| match x {
            0 => Profile::Ide,
            1 => Profile::Runtime,
            2 => Profile::Ai,
            _ => unreachable!("dialoguer returned an unknown profile index"),
        })
        .collect();

    print_step(3, 4, "Runtime artifacts");

    let registries = ["github", "gitlab", "custom"];
    let selected_registry = Select::with_theme(&theme)
        .with_prompt("Choose artifact registry")
        .items(registries)
        .default(0)
        .interact()?;

    let registry = match selected_registry {
        0 => Registry::GitHub.url().to_string(),
        1 => Registry::GitLab.url().to_string(),
        _ => Input::with_theme(&theme)
            .with_prompt("Custom image registry")
            .interact_text()?,
    };

    let version: String = Input::with_theme(&theme)
        .with_prompt("Version")
        .default("latest".into())
        .interact_text()?;

    print_step(4, 4, "Generate files");
    println!(
        "{}",
        style("Ready to write .codezero/.env and .codezero/docker-compose.yml").dim()
    );

    Ok(SetupProps {
        initial_username: username,
        initial_password: password,
        profiles: parsed_profiles,
        registry,
        version,
    })
}

impl From<SetupProps> for TemplateProps {
    fn from(props: SetupProps) -> Self {
        let compose_profiles = props
            .profiles
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",");

        Self {
            initial_root_mail: props.initial_username,
            initial_root_password: props.initial_password,
            initial_runtime_token: "runtime".into(),
            compose_profiles,
            image_registry: props.registry,
            image_tag: props.version,
            image_edition: "ce".into(),
            hostname: "localhost".into(),
            http_port: 80,
            https_port: 443,
            ssl_enabled: false,
            postgres_db: "sagittarius_production".into(),
            postgres_user: "sagittarius".into(),
            postgres_password: "sagittarius".into(),
            aquila_sagittarius_url: "http://nginx:80".into(),
            aquila_sagittarius_token: "runtime".into(),
            draco_rest_port: 8084,
            draco_rest_host: "127.0.0.1".into(),
            taurus_aquila_token: "taurus".into(),
            draco_rest_aquila_token: "draco-rest".into(),
            draco_cron_aquila_token: "draco-cron".into(),
            velorum_host: "velorum".into(),
            velorum_port: 50051,
            velorum_jwt_secret: "088cfc7a7fc2b07696d8ee5e1ea9d642".into(),
            sagittarius_rails_host: "sagittarius-rails-web".into(),
            sagittarius_rails_port: 3000,
            sagittarius_cable_host: "sagittarius-rails-cable".into(),
            sagittarius_cable_port: 3000,
            sagittarius_grpc_host: "sagittarius-grpc".into(),
            sagittarius_grpc_port: 50051,
            sagittarius_log_level: "info".into(),
            sculptor_host: "sculptor".into(),
            sculptor_port: 3000,
            postgres_host: "postgres".into(),
            postgres_port: 5432,
        }
    }
}

pub fn setup() -> anyhow::Result<()> {
    let props = collect_setup_props()?;
    let theme = wizard_theme();

    if fs::exists(".codezero")?
        && !Confirm::with_theme(&theme)
            .with_prompt("Overwrite existing .codezero configuration?")
            .default(false)
            .interact()?
    {
        println!("Setup cancelled.");
        return Ok(());
    }

    fs::create_dir_all(".codezero")?;

    let template_props = TemplateProps::from(props);
    render_setup_templates(".codezero", &template_props)?;

    println!();
    println!("{}", style("Configuration generated.").green().bold());
    println!(
        "{}",
        style("Starting CodeZero now. This can take a few minutes the first time.").dim()
    );

    default_runner().start()?;
    print_completion_links(&template_props);

    Ok(())
}

fn print_completion_links(props: &TemplateProps) {
    let scheme = if props.ssl_enabled { "https" } else { "http" };
    let port = if props.ssl_enabled {
        props.https_port
    } else {
        props.http_port
    };
    let app_url = format!("{scheme}://{}:{port}", props.hostname);

    println!();
    println!("{}", style("CodeZero is ready.").green().bold());
    println!("{} {}", style("Open:").bold(), style(app_url).cyan());
    println!(
        "{} {}",
        style("Docs:").bold(),
        style("https://docs.code0.tech").cyan()
    );
    println!(
        "{} {}",
        style("Bugs:").bold(),
        style("https://github.com/code0-tech/codezero/issues").cyan()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::render_template;

    fn template_props() -> TemplateProps {
        TemplateProps::from(SetupProps {
            initial_username: "root@code0.tech".into(),
            initial_password: "root".into(),
            profiles: vec![Profile::Ide, Profile::Runtime],
            registry: Registry::GitHub.url().into(),
            version: "latest".into(),
        })
    }

    #[test]
    fn renders_setup_templates() -> anyhow::Result<()> {
        let props = template_props();

        for template_path in ["templates/env.tmpl", "templates/docker-compose.yml.tmpl"] {
            render_template(template_path, &props)?;
        }

        Ok(())
    }
}
