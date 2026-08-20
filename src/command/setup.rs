use std::{collections::HashMap, fs, path::PathBuf};

use console::style;
use dialoguer::{Confirm, Input, MultiSelect, Password, Select, theme::ColorfulTheme};
use serde_json::{Map, Value};

use crate::{
    action::{SERVICE_CONFIGURATION_PATH, ServiceConfiguration, resolve_dev_tag},
    bundle::{BundleSource, Choice, SetupBundle, Step, default_choice, generate_token, join_multiselect},
    runner::{Runner, default_runner},
    template::render_setup_templates,
    ui,
};

const BANNER: &str = include_str!("../../assets/banner.txt");

fn print_banner(title: &str, subtitle: &str) {
    println!();
    for line in BANNER.lines() {
        println!("{}", ui::accent().apply_to(line));
    }
    println!();
    println!("{}", ui::secondary().bold().apply_to(title));
    println!("{}", ui::muted().apply_to(subtitle));
}

const CUSTOM_CHOICE_LABEL: &str = "custom";

fn run_select(
    theme: &ColorfulTheme,
    prompt: &str,
    options: &[Choice],
    allow_custom: bool,
    custom_prompt: &Option<String>,
    default_index: usize,
) -> anyhow::Result<Value> {
    let mut items: Vec<&str> = options.iter().map(|choice| choice.label.as_str()).collect();
    if allow_custom {
        items.push(CUSTOM_CHOICE_LABEL);
    }

    let selected = Select::with_theme(theme)
        .with_prompt(prompt)
        .items(&items)
        .default(default_index)
        .interact()?;

    if allow_custom && selected == options.len() {
        let prompt = custom_prompt.as_deref().unwrap_or("Custom value");
        let custom: String = Input::with_theme(theme).with_prompt(prompt).interact_text()?;
        return Ok(Value::String(custom));
    }

    Ok(options[selected].value.clone())
}

fn run_multiselect(
    theme: &ColorfulTheme,
    prompt: &str,
    options: &[Choice],
    join: &Option<String>,
    defaults: &[bool],
) -> anyhow::Result<Value> {
    let items: Vec<&str> = options.iter().map(|choice| choice.label.as_str()).collect();

    let selected_indices = MultiSelect::with_theme(theme)
        .with_prompt(prompt)
        .items(&items)
        .defaults(defaults)
        .interact()?;

    anyhow::ensure!(!selected_indices.is_empty(), "select at least one option");

    let selected_values: Vec<Value> = selected_indices
        .iter()
        .map(|&index| options[index].value.clone())
        .collect();

    Ok(join_multiselect(&selected_values, join))
}

/// The `.env`-style key a step's answer round-trips through, so a re-run of
/// the wizard (`configure`) can find the currently-running value and use it
/// as the prompt's default instead of falling back to the manifest default.
fn seeded_default(seed: Option<&HashMap<String, String>>, id: &str) -> Option<String> {
    seed.and_then(|env| env.get(&id.to_uppercase()).cloned())
}

/// The option (if any) whose value matches a seeded string, e.g. matching
/// `IMAGE_EDITION=ce` back to the "ce" choice so it's pre-selected.
fn matching_option_index(options: &[Choice], value: &str) -> Option<usize> {
    options.iter().position(|choice| display_value(&choice.value) == value)
}

/// Resolves every step's value in `--dev` mode without prompting: `image_tag`
/// specifically comes from reticulum's latest `main` build (the one step that
/// actually needs a live lookup); everything else uses the manifest's own
/// declared default — a step with nothing to fall back to is a manifest bug,
/// not something `--dev` can guess around.
fn auto_dev_value(step: &Step) -> anyhow::Result<Value> {
    if step.id() == "image_tag" {
        return Ok(Value::String(resolve_dev_tag()?));
    }

    match step {
        Step::Input { default, .. } => default
            .clone()
            .map(Value::String)
            .ok_or_else(|| anyhow::anyhow!("'{}' has no default for --dev", step.id())),
        Step::Password { default, .. } => default
            .clone()
            .map(Value::String)
            .ok_or_else(|| anyhow::anyhow!("'{}' has no default for --dev", step.id())),
        Step::Select { options, .. } => {
            let choice = default_choice(options)
                .ok_or_else(|| anyhow::anyhow!("'{}' step has no options to default to", step.id()))?;
            Ok(choice.value.clone())
        }
        Step::Multiselect { options, join, .. } => {
            let selected: Vec<Value> = options
                .iter()
                .filter(|choice| choice.default)
                .map(|choice| choice.value.clone())
                .collect();
            Ok(join_multiselect(&selected, join))
        }
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Runs the interactive (or `--dev` auto-resolved) setup wizard. `seed`, when
/// given, pre-fills every prompt with the value it currently has in
/// `.codezero/.env` (instead of the manifest's default) and reuses existing
/// generated secrets rather than rolling new ones - this is what lets
/// `configure` behave like "edit the current setup" rather than "start over".
pub(crate) fn run_wizard(
    bundle: &SetupBundle,
    theme: &ColorfulTheme,
    dev: bool,
    seed: Option<&HashMap<String, String>>,
) -> anyhow::Result<Map<String, Value>> {
    if seed.is_some() {
        print_banner("Reconfigure", "Update your existing .codezero configuration, then apply it.");
    } else {
        print_banner("Interactive setup", "Create a local .codezero runtime configuration, then start it.");
    }

    let sections: Vec<&str> = {
        let mut seen = Vec::new();
        for step in &bundle.steps {
            if !seen.contains(&step.section()) {
                seen.push(step.section());
            }
        }
        seen
    };
    let total_sections = sections.len() + if bundle.secrets.is_empty() { 0 } else { 1 };

    let mut context = bundle.static_values.clone();
    let mut current_section: Option<&str> = None;

    for step in &bundle.steps {
        if current_section != Some(step.section()) {
            let index = sections
                .iter()
                .position(|section| *section == step.section())
                .unwrap_or(0)
                + 1;
            ui::section(index, total_sections, step.section());
            current_section = Some(step.section());
        }

        if dev {
            let value = auto_dev_value(step)?;
            ui::success_line(&format!("{}: {}", step.id(), display_value(&value)));
            context.insert(step.id().to_string(), value);
            continue;
        }

        let (id, value) = match step {
            Step::Input {
                id,
                prompt,
                default,
                ..
            } => {
                let seeded = seeded_default(seed, id);
                let mut input = Input::with_theme(theme).with_prompt(prompt);
                if let Some(default) = seeded.or_else(|| default.clone()) {
                    input = input.default(default);
                }
                let value: String = input.interact_text()?;
                (id.clone(), Value::String(value))
            }
            Step::Password {
                id,
                prompt,
                confirm,
                ..
            } => {
                let seeded = seeded_default(seed, id);
                let display_prompt = if seeded.is_some() {
                    format!("{prompt} (leave blank to keep current)")
                } else {
                    prompt.clone()
                };
                let mut password = Password::with_theme(theme).with_prompt(display_prompt);
                if *confirm {
                    password = password.with_confirmation(
                        format!("Confirm {}", prompt.to_lowercase()),
                        "Passwords do not match",
                    );
                }
                let typed = password.interact()?;
                // dialoguer's password prompt has no notion of a displayed
                // default, so "leave blank to keep current" is handled here
                // instead: an empty answer falls back to the seeded value.
                let value = if typed.is_empty() { seeded.unwrap_or(typed) } else { typed };
                (id.clone(), Value::String(value))
            }
            Step::Select {
                id,
                prompt,
                options,
                allow_custom,
                custom_prompt,
                ..
            } => {
                let default_index = seeded_default(seed, id)
                    .and_then(|value| matching_option_index(options, &value))
                    .or_else(|| options.iter().position(|choice| choice.default))
                    .unwrap_or(0);
                let value = run_select(theme, prompt, options, *allow_custom, custom_prompt, default_index)?;
                (id.clone(), value)
            }
            Step::Multiselect {
                id,
                prompt,
                options,
                join,
                ..
            } => {
                let defaults: Vec<bool> = match seeded_default(seed, id) {
                    Some(seeded) => {
                        let selected: Vec<&str> = match join {
                            Some(separator) => seeded.split(separator.as_str()).collect(),
                            None => vec![seeded.as_str()],
                        };
                        options
                            .iter()
                            .map(|choice| selected.contains(&display_value(&choice.value).as_str()))
                            .collect()
                    }
                    None => options.iter().map(|choice| choice.default).collect(),
                };
                let value = run_multiselect(theme, prompt, options, join, &defaults)?;
                (id.clone(), value)
            }
        };

        context.insert(id, value);
    }

    if !bundle.secrets.is_empty() {
        ui::section(total_sections, total_sections, "Secrets");

        for secret in &bundle.secrets {
            let primary_key = secret.id.as_slice()[0].to_uppercase();
            let value = match seed.and_then(|env| env.get(&primary_key)) {
                Some(existing) => existing.clone(),
                None => generate_token(secret.length),
            };
            for key in secret.id.as_slice() {
                context.insert(key.to_string(), Value::String(value.clone()));
            }

            let label = secret
                .label
                .clone()
                .unwrap_or_else(|| secret.id.as_slice().join(", "));
            let verb = if seed.is_some() { "Kept" } else { "Generated" };
            ui::success_line(&format!("{verb} {label}"));
        }
    }

    println!();
    ui::muted_line("Ready to write .codezero/.env and .codezero/docker-compose.yml");

    Ok(context)
}

fn context_str<'a>(context: &'a Map<String, Value>, key: &str) -> anyhow::Result<&'a str> {
    context
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("setup bundle context is missing string field '{key}'"))
}

fn context_u16(context: &Map<String, Value>, key: &str) -> anyhow::Result<u16> {
    context
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| anyhow::anyhow!("setup bundle context is missing numeric field '{key}'"))
}

fn context_bool(context: &Map<String, Value>, key: &str) -> anyhow::Result<bool> {
    context
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("setup bundle context is missing boolean field '{key}'"))
}

pub fn setup(bundle_path: Option<PathBuf>, dev: bool) -> anyhow::Result<()> {
    let source = BundleSource::from_arg(bundle_path);
    let bundle = SetupBundle::load(&source)?;
    let theme = ui::wizard_theme();

    let context = run_wizard(&bundle, &theme, dev, None)?;

    if fs::exists(".codezero")?
        && !Confirm::with_theme(&theme)
            .with_prompt("Overwrite existing .codezero configuration?")
            .default(false)
            .interact()?
    {
        ui::warn_line("Setup cancelled.");
        return Ok(());
    }

    // service.configuration.json is one of the rendered templates, but it's
    // also the file `codezero install`/`uninstall` maintain — regenerating it
    // from scratch would silently wipe out any installed actions. Carry them
    // forward across the fresh render (runtimes tokens still get refreshed).
    let installed_actions = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?.actions;

    fs::create_dir_all(".codezero")?;
    render_setup_templates(&source, &bundle.templates, &context, ".codezero")?;

    if !installed_actions.is_empty() {
        let mut config = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?;
        config.actions = installed_actions;
        config.save(SERVICE_CONFIGURATION_PATH)?;
    }

    println!();
    ui::success_line("Configuration generated.");
    ui::muted_line("Starting CodeZero now. This can take a few minutes the first time.");

    default_runner().start()?;
    print_completion_links(&context)?;

    Ok(())
}

fn print_completion_links(context: &Map<String, Value>) -> anyhow::Result<()> {
    let ssl_enabled = context_bool(context, "ssl_enabled")?;
    let scheme = if ssl_enabled { "https" } else { "http" };
    let port = if ssl_enabled {
        context_u16(context, "https_port")?
    } else {
        context_u16(context, "http_port")?
    };
    let hostname = context_str(context, "hostname")?;
    let app_url = format!("{scheme}://{hostname}:{port}");

    println!();
    ui::success_line("CodeZero is ready.");
    println!("  {} {}", style("Open:").bold(), ui::accent().apply_to(app_url));
    println!(
        "  {} {}",
        style("Docs:").bold(),
        ui::accent().apply_to("https://docs.code0.tech")
    );
    println!(
        "  {} {}",
        style("Bugs:").bold(),
        ui::accent().apply_to("https://github.com/code0-tech/codezero/issues")
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::render_template;

    fn fixture_context() -> Map<String, Value> {
        let entries: Vec<(&str, Value)> = vec![
            ("initial_root_mail", "root@code0.tech".into()),
            ("initial_root_password", "root".into()),
            ("initial_runtime_token", "runtime".into()),
            ("compose_profiles", "ide,runtime".into()),
            ("image_registry", "ghcr.io/code0-tech/reticulum/ci-builds".into()),
            ("image_tag", "latest".into()),
            ("image_edition", "ce".into()),
            ("hostname", "localhost".into()),
            ("http_port", 80.into()),
            ("https_port", 443.into()),
            ("ssl_enabled", false.into()),
            ("ssl_cert_file", "".into()),
            ("ssl_key_file", "".into()),
            ("postgres_db", "sagittarius_production".into()),
            ("postgres_user", "sagittarius".into()),
            ("postgres_password", "sagittarius".into()),
            ("postgres_host", "postgres".into()),
            ("postgres_port", 5432.into()),
            ("aquila_log_level", "info".into()),
            ("aquila_nats_url", "nats://nats:4222".into()),
            ("aquila_nats_bucket", "flow_store".into()),
            ("aquila_backend_url", "http://nginx:80".into()),
            ("aquila_backend_token", "runtime".into()),
            ("aquila_backend_unary_timeout_secs", 10.into()),
            ("aquila_grpc_host", "0.0.0.0".into()),
            ("aquila_grpc_port", 8081.into()),
            ("aquila_grpc_health_service", false.into()),
            ("aquila_grpc_connect_timeout_secs", 2.into()),
            ("aquila_grpc_request_timeout_secs", 10.into()),
            ("rest_action_port", 8084.into()),
            ("aquila_action_rest_identifier", "rest-action".into()),
            ("aquila_action_cron_identifier", "cron-action".into()),
            ("action_image_registry", "ghcr.io/code0-tech/centaurus/ci-builds".into()),
            ("action_image_tag", "latest".into()),
            ("taurus_aquila_token", "taurus".into()),
            ("aquila_action_rest_token", "rest-action-token".into()),
            ("aquila_action_cron_token", "cron-action-token".into()),
            ("velorum_host", "velorum".into()),
            ("velorum_port", 50051.into()),
            ("velorum_jwt_secret", "secret".into()),
            ("sagittarius_db_encryption_primary_key", "primary-key".into()),
            ("sagittarius_db_encryption_deterministic_key", "deterministic-key".into()),
            ("sagittarius_db_encryption_key_derivation_salt", "salt".into()),
            ("sagittarius_rails_secret_key_base", "secret-key-base".into()),
            ("sagittarius_gateway_jwt_secret", "gateway-secret".into()),
            ("sagittarius_gateway_jwt_ttl_seconds", 300.into()),
            ("sagittarius_gateway_log_level", "info".into()),
            ("opentelemetry_enabled", false.into()),
            ("opentelemetry_grpc_host", "http://localhost:4317".into()),
            ("opentelemetry_logs_http_endpoint", "http://localhost:4318/v1/logs".into()),
            ("opentelemetry_metrics_http_endpoint", "http://localhost:4318/v1/metrics".into()),
            ("opentelemetry_traces_http_endpoint", "http://localhost:4318/v1/traces".into()),
            ("opentelemetry_logs_clientside_http_endpoint", "http://localhost:4318/v1/logs".into()),
            ("opentelemetry_metrics_clientside_http_endpoint", "http://localhost:4318/v1/metrics".into()),
            ("opentelemetry_traces_clientside_http_endpoint", "http://localhost:4318/v1/traces".into()),
            ("opentelemetry_service_name_sagittarius_web", "sagittarius-web".into()),
            ("opentelemetry_service_name_sagittarius_cable", "sagittarius-cable".into()),
            ("opentelemetry_service_name_sagittarius_grpc", "sagittarius-grpc".into()),
            ("opentelemetry_service_name_sagittarius_background", "sagittarius-background".into()),
            ("opentelemetry_service_name_sagittarius_gateway", "sagittarius-gateway".into()),
            ("opentelemetry_service_name_nginx", "nginx".into()),
            ("opentelemetry_service_name_sculptor", "sculptor".into()),
            ("opentelemetry_service_name_sculptor_clientside", "sculptor-clientside".into()),
            ("sagittarius_rails_host", "sagittarius-rails-web".into()),
            ("sagittarius_rails_port", 3000.into()),
            ("sagittarius_cable_host", "sagittarius-rails-cable".into()),
            ("sagittarius_cable_port", 3000.into()),
            ("sagittarius_grpc_host", "sagittarius-grpc".into()),
            ("sagittarius_grpc_port", 50051.into()),
            ("sagittarius_gateway_host", "sagittarius-gateway".into()),
            ("sagittarius_gateway_port", 50051.into()),
            ("sagittarius_log_level", "info".into()),
            ("sagittarius_rails_web_threads", 3.into()),
            ("sagittarius_rails_grpc_threads", 6.into()),
            ("sagittarius_db_pool_size", 8.into()),
            ("sculptor_host", "sculptor".into()),
            ("sculptor_port", 3000.into()),
        ];

        entries.into_iter().map(|(key, value)| (key.to_string(), value)).collect()
    }

    #[test]
    fn renders_setup_templates() -> anyhow::Result<()> {
        let context = fixture_context();

        for name in ["env.tera", "docker-compose.yml", "service.configuration.json.tera"] {
            let raw = crate::embedded::template(name).expect("template should be embedded");
            render_template(raw, &context)?;
        }

        Ok(())
    }

    #[test]
    fn print_completion_links_reads_scheme_and_port_from_context() -> anyhow::Result<()> {
        let context = fixture_context();
        print_completion_links(&context)
    }

    #[test]
    fn auto_dev_value_picks_the_default_registry_choice() {
        let step = Step::Select {
            id: "image_registry".into(),
            section: "Runtime artifacts".into(),
            prompt: "Choose artifact registry".into(),
            options: vec![
                Choice { label: "github".into(), value: Value::String("ghcr.io/code0-tech/reticulum/ci-builds".into()), default: false },
                Choice { label: "gitlab".into(), value: Value::String("registry.gitlab.com/code0-tech/packages".into()), default: false },
            ],
            allow_custom: true,
            custom_prompt: Some("Custom image registry".into()),
        };

        let value = auto_dev_value(&step).expect("registry step should auto-resolve");
        assert_eq!(value, "ghcr.io/code0-tech/reticulum/ci-builds");
    }

    #[test]
    fn auto_dev_value_uses_input_default() {
        let step = Step::Input {
            id: "initial_root_mail".into(),
            section: "Admin account".into(),
            prompt: "Root email".into(),
            default: Some("root@code0.tech".into()),
        };

        let value = auto_dev_value(&step).expect("input step should auto-resolve");
        assert_eq!(value, "root@code0.tech");
    }

    #[test]
    fn auto_dev_value_uses_password_default() {
        let step = Step::Password {
            id: "initial_root_password".into(),
            section: "Admin account".into(),
            prompt: "Root password".into(),
            confirm: true,
            default: Some("root".into()),
        };

        let value = auto_dev_value(&step).expect("password step should auto-resolve");
        assert_eq!(value, "root");
    }

    #[test]
    fn auto_dev_value_errors_without_a_default() {
        let step = Step::Input {
            id: "no_default".into(),
            section: "Admin account".into(),
            prompt: "Something".into(),
            default: None,
        };

        assert!(auto_dev_value(&step).is_err());
    }

    #[test]
    fn auto_dev_value_selects_default_flagged_multiselect_options() {
        let step = Step::Multiselect {
            id: "compose_profiles".into(),
            section: "Runtime profiles".into(),
            prompt: "Choose profiles".into(),
            options: vec![
                Choice { label: "ide".into(), value: Value::String("ide".into()), default: true },
                Choice { label: "runtime".into(), value: Value::String("runtime".into()), default: true },
                Choice { label: "ai".into(), value: Value::String("ide_velorum".into()), default: false },
            ],
            join: Some(",".into()),
        };

        let value = auto_dev_value(&step).expect("multiselect step should auto-resolve");
        assert_eq!(value, "ide,runtime");
    }
}
