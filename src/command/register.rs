use std::fs;

use crate::{
    action::{SERVICE_CONFIGURATION_PATH, ServiceConfiguration},
    bundle::generate_token,
    env_file,
    runner::{default_runner, refresh_aquila_config},
    ui,
};

const ENV_PATH: &str = ".codezero/.env";

/// Registers an arbitrary identifier with aquila without managing a
/// container for it - for developing a new action locally: run it yourself
/// (`npm run dev`, `cargo run`, ...) pointed at the printed connection
/// details instead of waiting on an image build/publish/`plugin install`
/// round-trip. Unlike `install`, there's no centaurus catalog lookup - the
/// identifier is whatever you want to call your in-progress action.
pub fn register(identifier: String) -> anyhow::Result<()> {
    if !fs::exists(ENV_PATH)? {
        anyhow::bail!("No CodeZero setup found. Run `codezero setup` first.");
    }

    let token = generate_token(64);

    let mut config = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?;
    let updated = config.upsert_action(&identifier, &token);
    config.save(SERVICE_CONFIGURATION_PATH)?;

    match refresh_aquila_config(&default_runner()) {
        Ok(()) => ui::success_line(&format!(
            "{} '{identifier}' with Aquila.",
            if updated { "Re-registered" } else { "Registered" }
        )),
        Err(error) => ui::warn_line(&format!(
            "Couldn't apply the change automatically ({error}). Run `codezero start` to apply."
        )),
    }

    // Aquila's gRPC port is published to the host (see `docker-compose.yml`'s
    // `ports: - "${AQUILA_GRPC_PORT}:8081"`), so a locally-running action
    // reaches it via localhost, not the in-network `aquila` hostname.
    let port = env_file::read_value(ENV_PATH, "AQUILA_GRPC_PORT")?.unwrap_or_else(|| "8081".to_string());

    println!();
    ui::muted_line("Point your locally-running action at:");
    println!("  AQUILA_URL={}", ui::accent().apply_to(format!("localhost:{port}")));
    println!("  AUTH_TOKEN={}", ui::accent().apply_to(&token));

    Ok(())
}

/// Removes a previously `register`ed identifier. No-op (with a warning) if
/// it wasn't registered - matches `uninstall`'s behavior for an action that
/// was never installed.
pub fn unregister(identifier: String) -> anyhow::Result<()> {
    let mut config = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?;
    let removed = config.remove_action(&identifier);

    if !removed {
        ui::warn_line(&format!("'{identifier}' was not registered."));
        return Ok(());
    }

    config.save(SERVICE_CONFIGURATION_PATH)?;

    match refresh_aquila_config(&default_runner()) {
        Ok(()) => ui::success_line(&format!("Unregistered '{identifier}'.")),
        Err(error) => ui::warn_line(&format!(
            "Couldn't apply the change automatically ({error}). Run `codezero start` to apply."
        )),
    }

    Ok(())
}
