use std::{fs, path::PathBuf};

use crate::{
    action::{ActionCatalog, SERVICE_CONFIGURATION_PATH, ServiceConfiguration, action_fragment_path},
    runner::{Runner, default_runner},
    ui,
};

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

    ui::success_line(&format!("Uninstalled {}.", entry.identifier));

    match default_runner().restart("aquila") {
        Ok(()) => ui::success_line("Restarted Aquila to apply the change."),
        Err(error) => ui::warn_line(&format!(
            "Couldn't restart Aquila automatically ({error}). Run `codezero start` to apply."
        )),
    }

    Ok(())
}
