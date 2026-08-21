use std::fs;

use crate::{
    command::setup,
    runner::{Runner, default_runner},
    ui,
};

pub fn reset() -> anyhow::Result<()> {
    if fs::exists(".codezero")? {
        // Tear down containers and volumes first, while .codezero/.env and
        // docker-compose.yml still exist for `docker compose down` to read.
        // Skipping this would leave stale volumes (e.g. config-generator's
        // already-completed, restart:"no" container and its generated
        // config files) around for the next `start` to silently reuse.
        if let Err(error) = default_runner().stop(true) {
            ui::warn_line(&format!(
                "Couldn't stop the running stack automatically ({error}). Continuing with reset."
            ));
        }

        fs::remove_dir_all(".codezero")?;
        ui::warn_line("Removed existing .codezero configuration.");
    }

    setup::setup(None, false)
}
