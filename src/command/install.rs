use std::{fs, path::PathBuf};

use crate::{
    action::{
        ACTIONS_DIR, ActionCatalog, ActionEntry, SERVICE_CONFIGURATION_PATH, ServiceConfiguration,
        action_fragment_path, parse_install_spec,
    },
    bundle::generate_token,
    runner::{Runner, default_runner},
    ui,
};

pub fn install(index_path: Option<PathBuf>, spec: String) -> anyhow::Result<()> {
    let (name, tag_override) = parse_install_spec(&spec);
    let catalog = ActionCatalog::load(index_path.as_deref())?;
    let entry = catalog.find(name).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown action '{name}'. Available: {}",
            catalog.names().join(", ")
        )
    })?;
    let tag = tag_override.unwrap_or(entry.version.as_str());
    let token = generate_token(64);

    let mut config = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?;
    let updated = config.upsert_action(&entry.identifier, &token);
    config.save(SERVICE_CONFIGURATION_PATH)?;

    write_action_compose_fragment(entry, tag, &token)?;

    ui::success_line(&format!(
        "{} {} ({tag})",
        if updated { "Updated" } else { "Installed" },
        entry.identifier
    ));

    let runner = default_runner();
    match runner
        .start_service(&entry.identifier)
        .and_then(|()| runner.restart("aquila"))
    {
        Ok(()) => ui::success_line("Restarted Aquila to apply the new configuration."),
        Err(error) => ui::warn_line(&format!(
            "Couldn't apply the change automatically ({error}). Run `codezero start` to apply."
        )),
    }

    Ok(())
}

fn write_action_compose_fragment(entry: &ActionEntry, tag: &str, token: &str) -> anyhow::Result<()> {
    fs::create_dir_all(ACTIONS_DIR)?;

    let docker = &entry.deployment.docker;
    // No scheme prefix here: hercules' Action SDK passes this straight into a
    // gRPC channel target (bare "host:port", like the SDK's own docs example
    // 'aquila.example.com:8081'), not a URL. An "http://" prefix produces a
    // malformed resolver target ("dns:http://aquila:8081") that only
    // resolves by accident.
    let compose = format!(
        "name: codezero\nservices:\n  {identifier}:\n    image: {image}:{tag}\n    depends_on:\n      aquila:\n        condition: service_started\n    environment:\n      {aquila_url_var}: aquila:8081\n      {auth_token_var}: {token}\n    restart: unless-stopped\n    profiles:\n      - runtime\n",
        identifier = entry.identifier,
        image = docker.image,
        aquila_url_var = docker.aquila_url_var,
        auth_token_var = docker.auth_token_var,
    );

    fs::write(action_fragment_path(&entry.identifier), compose)?;

    Ok(())
}
