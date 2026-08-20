use std::{fs, path::PathBuf};

use crate::{
    action::{SERVICE_CONFIGURATION_PATH, ServiceConfiguration},
    bundle::{BundleSource, SetupBundle},
    command::setup::run_wizard,
    env_file,
    runner::{Runner, default_runner},
    template::render_setup_templates,
    ui,
};

const ENV_PATH: &str = ".codezero/.env";

/// Re-runs the setup wizard against an existing install, seeded with its
/// current answers, instead of `reset`'s destructive wipe-and-start-over.
/// Existing secrets are kept as-is (not regenerated), and only whatever
/// actually changed gets recreated when the stack is brought back up.
pub fn configure(bundle_path: Option<PathBuf>) -> anyhow::Result<()> {
    if !fs::exists(ENV_PATH)? {
        anyhow::bail!("No CodeZero setup found. Run `codezero setup` first.");
    }

    let source = BundleSource::from_arg(bundle_path);
    let bundle = SetupBundle::load(&source)?;
    let theme = ui::wizard_theme();
    let seed = env_file::read_all(ENV_PATH)?;

    let context = run_wizard(&bundle, &theme, false, Some(&seed))?;

    // Same carry-forward as `setup`: the rendered service.configuration.json
    // starts from an empty action list, so installed actions have to be
    // reapplied after rendering or `install`/`uninstall` state would be lost.
    let installed_actions = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?.actions;

    render_setup_templates(&source, &bundle.templates, &context, ".codezero")?;

    if !installed_actions.is_empty() {
        let mut config = ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?;
        config.actions = installed_actions;
        config.save(SERVICE_CONFIGURATION_PATH)?;
    }

    println!();
    ui::muted_line("Applying changes...");
    default_runner().start()?;

    ui::success_line("CodeZero reconfigured.");
    Ok(())
}
