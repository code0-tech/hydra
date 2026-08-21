use std::{fs, path::PathBuf};

use crate::{
    action::{ActionCatalog, SERVICE_CONFIGURATION_PATH, ServiceConfiguration, action_fragment_path, aquila_action_env_prefix},
    env_file,
    runner::{Runner, default_runner, refresh_aquila_config},
    ui,
};

const ENV_PATH: &str = ".codezero/.env";

pub fn uninstall(index_path: Option<PathBuf>, name: String) -> anyhow::Result<()> {
    let catalog = ActionCatalog::load(index_path.as_deref())?;
    let entry = catalog.find(&name).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown action '{name}'. Available: {}",
            catalog.names().join(", ")
        )
    })?;

    let fragment_path = action_fragment_path(&entry.identifier);
    let was_installed = fragment_path.exists();

    if was_installed {
        // Stop and remove the container while the fragment file (which
        // defines the service to compose) still exists on disk.
        if let Err(error) = default_runner().stop_service(&entry.identifier) {
            ui::warn_line(&format!(
                "Couldn't stop {} automatically ({error}). Continuing with removal.",
                entry.identifier
            ));
        }
        fs::remove_file(&fragment_path)?;
    }

    let mut config = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?;
    let removed_from_config = config.remove_action(&entry.identifier);
    if removed_from_config {
        config.save(SERVICE_CONFIGURATION_PATH)?;
    }

    if !was_installed && !removed_from_config {
        ui::warn_line(&format!("{} was not installed.", entry.identifier));
        return Ok(());
    }

    // Revoke the token aquila actually trusts (see `install.rs` - it's this
    // pair in `.env`, not `.codezero/service.configuration.json`, that
    // config-generator turns into aquila's accepted-token list).
    let prefix = aquila_action_env_prefix(&entry.identifier);
    if fs::exists(ENV_PATH)? {
        env_file::remove_keys(ENV_PATH, &[&format!("{prefix}_IDENTIFIER"), &format!("{prefix}_TOKEN")])?;
    }

    ui::success_line(&format!("Uninstalled {}.", entry.identifier));

    match refresh_aquila_config(&default_runner()) {
        Ok(()) => ui::success_line("Restarted Aquila to apply the change."),
        Err(error) => ui::warn_line(&format!(
            "Couldn't restart Aquila automatically ({error}). Run `codezero start` to apply."
        )),
    }

    Ok(())
}
