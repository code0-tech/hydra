use std::{collections::HashMap, fs, path::PathBuf};

use dialoguer::Confirm;
use serde_json::{Map, Value};

use crate::{
    action::{
        SERVICE_CONFIGURATION_PATH, ServiceConfiguration, carry_forward_actions,
        resolve_action_dev_tag, resolve_dev_tag,
    },
    bundle::{BundleSource, SetupBundle},
    env_file,
    runner::{Runner, default_runner},
    template::render_setup_templates,
    ui, update_check,
};

const ENV_PATH: &str = ".codezero/.env";

/// Hard-updates an existing install: refetches the reticulum bundle, bumps the
/// core *and* action image tags to the chosen build, and restarts whatever
/// changed — without a destructive `reset`.
///
/// This is the experimental `--upgrade` entry point surfaced by the "new
/// version available" reminder, not a standalone command yet (a dedicated
/// `upgrade` command will replace it later). The target build is opt-in: pin it
/// with `--tag`, or take the newest in-progress reticulum build with `--dev` —
/// there's no stable release channel to default to, so one of the two is
/// required.
///
/// Unlike `configure`, this re-renders non-interactively and seeds the render
/// from the *entire* existing `.env` (see `build_upgrade_context`), so every
/// setting the user already has — secrets, admin account, installed-action
/// tokens — survives the refreshed bundle; only genuinely new keys reticulum
/// introduced pick up a default. Gated behind an explicit confirm since a new
/// bundle can bring breaking changes.
pub fn upgrade(
    tag: Option<String>,
    registry: Option<String>,
    edition: Option<String>,
    bundle: Option<PathBuf>,
    dev: bool,
) -> anyhow::Result<()> {
    if !fs::exists(ENV_PATH)? {
        anyhow::bail!("No CodeZero setup found. Run `codezero setup` first.");
    }

    // Core and action images are built by different repos (reticulum vs
    // centaurus) on independent pipeline-id counters, so `--dev` resolves each
    // against its own project. An explicit `--tag` pins both to the same build
    // the user asked for.
    let (core_target, action_target) = match tag {
        Some(tag) => (tag.clone(), tag),
        None if dev => {
            ui::muted_line("Resolving the latest CodeZero build...");
            (resolve_dev_tag()?, resolve_action_dev_tag()?)
        }
        None => anyhow::bail!(
            "Pick a build to upgrade to: pass `--tag <version>`, or `--dev` for the latest in-progress build."
        ),
    };

    if !confirm_upgrade(
        &core_target,
        &action_target,
        registry.as_deref(),
        edition.as_deref(),
    )? {
        ui::warn_line("Upgrade cancelled.");
        return Ok(());
    }

    let source = BundleSource::from_arg(bundle);
    let bundle = SetupBundle::load(&source)?;

    let seed = env_file::read_all(ENV_PATH)?;
    let context = build_upgrade_context(
        &bundle.static_values,
        &seed,
        &core_target,
        &action_target,
        registry.as_deref(),
        edition.as_deref(),
    );

    // Same carry-forward as `setup`/`configure`: the rendered
    // service.configuration.json starts fresh from the bundle's template, so
    // anything `plugin install` added has to be reapplied afterward.
    let installed_actions =
        ServiceConfiguration::load_or_default(SERVICE_CONFIGURATION_PATH)?.actions;

    render_setup_templates(&source, &bundle.templates, &context, ".codezero")?;
    carry_forward_actions(installed_actions)?;

    println!();
    ui::muted_line("Pulling new images and restarting whatever changed...");
    default_runner().start()?;

    // Clear any pending "new build available" reminder right away rather than
    // waiting for the next background check to catch up.
    update_check::record_current_tag(&core_target, &action_target);

    ui::success_line(&format!(
        "CodeZero upgraded to {core_target} (actions {action_target})."
    ));
    Ok(())
}

/// Warns that an upgrade can bring breaking changes and spells out exactly what
/// is about to change, then asks for an explicit yes (defaulting to no).
fn confirm_upgrade(
    core_target: &str,
    action_target: &str,
    registry: Option<&str>,
    edition: Option<&str>,
) -> anyhow::Result<bool> {
    let current_core = env_file::read_value(ENV_PATH, "IMAGE_TAG")?.unwrap_or_default();
    let current_actions = env_file::read_value(ENV_PATH, "ACTION_IMAGE_TAG")?.unwrap_or_default();

    ui::warn_line("This refetches the latest bundle and may include breaking changes.");
    ui::muted_line(&format!("  core platform:  {current_core} -> {core_target}"));
    ui::muted_line(&format!("  actions:        {current_actions} -> {action_target}"));
    if let Some(registry) = registry {
        ui::muted_line(&format!("  image registry: {registry}"));
    }
    if let Some(edition) = edition {
        ui::muted_line(&format!("  image edition:  {edition}"));
    }
    ui::muted_line("Your secrets, admin account, and installed actions are kept.");

    Ok(Confirm::with_theme(&ui::wizard_theme())
        .with_prompt("Continue with the upgrade?")
        .default(false)
        .interact()?)
}

/// Builds the render context for a hard update. Starts from the new bundle's
/// `static_values` (so genuinely-new keys reticulum added get their default),
/// then overlays *every* value already in `.env` — lowercased to the context
/// key convention — so nothing the user has is lost; finally forces the image
/// tags (and optional registry/edition overrides) to the upgrade target.
///
/// Carrying the whole `.env` forward (rather than just wizard steps, as
/// `configure` does) is what keeps installed-action pairs like
/// `AQUILA_ACTION_GLS_TOKEN` from being dropped when the vendored `.env` is
/// refetched.
fn build_upgrade_context(
    static_values: &Map<String, Value>,
    seed: &HashMap<String, String>,
    core_target: &str,
    action_target: &str,
    registry: Option<&str>,
    edition: Option<&str>,
) -> Map<String, Value> {
    let mut context = static_values.clone();

    for (key, value) in seed {
        context.insert(key.to_lowercase(), Value::String(value.clone()));
    }

    context.insert("image_tag".into(), Value::String(core_target.into()));
    context.insert("action_image_tag".into(), Value::String(action_target.into()));
    if let Some(registry) = registry {
        context.insert("image_registry".into(), Value::String(registry.into()));
    }
    if let Some(edition) = edition {
        context.insert("image_edition".into(), Value::String(edition.into()));
    }

    context
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> HashMap<String, String> {
        [
            ("IMAGE_TAG", "0.0.0-experimental-100-aaa"),
            ("ACTION_IMAGE_TAG", "0.0.0-experimental-100-aaa"),
            ("INITIAL_ROOT_PASSWORD", "hunter2"),
            ("AQUILA_ACTION_GLS_IDENTIFIER", "gls-action"),
            ("AQUILA_ACTION_GLS_TOKEN", "gls-secret"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
    }

    #[test]
    fn carries_every_existing_env_value_forward_lowercased() {
        let context = build_upgrade_context(
            &Map::new(),
            &seed(),
            "0.0.0-experimental-200-bbb",
            "0.0.0-experimental-200-bbb",
            None,
            None,
        );

        // An installed-action token (not a wizard step) must survive the render.
        assert_eq!(
            context.get("aquila_action_gls_token"),
            Some(&Value::String("gls-secret".into()))
        );
        assert_eq!(
            context.get("initial_root_password"),
            Some(&Value::String("hunter2".into()))
        );
    }

    #[test]
    fn sets_each_image_tag_from_its_own_target() {
        let core = "0.0.0-experimental-200-bbb";
        let action = "0.0.0-experimental-777-ccc";
        let context = build_upgrade_context(&Map::new(), &seed(), core, action, None, None);

        assert_eq!(context.get("image_tag"), Some(&Value::String(core.into())));
        assert_eq!(
            context.get("action_image_tag"),
            Some(&Value::String(action.into()))
        );
    }

    #[test]
    fn fills_new_bundle_defaults_the_seed_does_not_have() {
        let mut static_values = Map::new();
        static_values.insert("brand_new_key".into(), Value::String("default".into()));

        let context = build_upgrade_context(&static_values, &seed(), "t", "t", None, None);

        assert_eq!(
            context.get("brand_new_key"),
            Some(&Value::String("default".into()))
        );
    }

    #[test]
    fn seed_wins_over_static_defaults_for_overlapping_keys() {
        let mut static_values = Map::new();
        static_values.insert("initial_root_password".into(), Value::String("default".into()));

        let context = build_upgrade_context(&static_values, &seed(), "t", "t", None, None);

        assert_eq!(
            context.get("initial_root_password"),
            Some(&Value::String("hunter2".into()))
        );
    }

    #[test]
    fn applies_registry_and_edition_only_when_provided() {
        let without = build_upgrade_context(&Map::new(), &seed(), "t", "t", None, None);
        assert!(!without.contains_key("image_registry"));
        assert!(!without.contains_key("image_edition"));

        let with =
            build_upgrade_context(&Map::new(), &seed(), "t", "t", Some("ghcr.io/x"), Some("cc"));
        assert_eq!(
            with.get("image_registry"),
            Some(&Value::String("ghcr.io/x".into()))
        );
        assert_eq!(with.get("image_edition"), Some(&Value::String("cc".into())));
    }
}
