use std::fs;

use dialoguer::Input;

use crate::{
    action::resolve_dev_tag,
    env_file,
    runner::{Runner, default_runner},
    ui,
};

const ENV_PATH: &str = ".codezero/.env";

/// Bumps the image version CodeZero runs without the destructive `reset`
/// round-trip: `reset` wipes `.codezero` and re-runs the whole wizard,
/// generating fresh passwords/secrets along the way, just to change one
/// version string. This edits only the image-related keys in the existing
/// `.env` and lets `docker compose up -d` recreate whichever services
/// actually changed.
pub fn upgrade(tag: Option<String>, registry: Option<String>, edition: Option<String>, dev: bool) -> anyhow::Result<()> {
    if !fs::exists(ENV_PATH)? {
        anyhow::bail!("No CodeZero setup found. Run `codezero setup` first.");
    }

    let new_tag = if let Some(tag) = tag {
        tag
    } else if dev {
        ui::muted_line("Resolving the latest reticulum build...");
        resolve_dev_tag()?
    } else {
        let current_tag = env_file::read_value(ENV_PATH, "IMAGE_TAG")?
            .ok_or_else(|| anyhow::anyhow!("Couldn't find IMAGE_TAG in {ENV_PATH}"))?;

        Input::with_theme(&ui::wizard_theme())
            .with_prompt("Version")
            .default(current_tag)
            .interact_text()?
    };

    let mut updates = vec![("IMAGE_TAG", new_tag.as_str())];
    if let Some(registry) = &registry {
        updates.push(("IMAGE_REGISTRY", registry.as_str()));
    }
    if let Some(edition) = &edition {
        updates.push(("IMAGE_EDITION", edition.as_str()));
    }

    env_file::set_values(ENV_PATH, &updates)?;

    ui::success_line(&format!("Set IMAGE_TAG to {new_tag}"));
    if let Some(registry) = &registry {
        ui::success_line(&format!("Set IMAGE_REGISTRY to {registry}"));
    }
    if let Some(edition) = &edition {
        ui::success_line(&format!("Set IMAGE_EDITION to {edition}"));
    }

    println!();
    ui::muted_line("Pulling new images and restarting whatever changed...");
    default_runner().start()?;

    ui::success_line("CodeZero upgraded.");
    Ok(())
}
