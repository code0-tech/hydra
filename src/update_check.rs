use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{action::resolve_dev_tag, env_file, ui};

const ENV_PATH: &str = ".codezero/.env";
const CACHE_PATH: &str = ".codezero/update-check.json";

/// How long a cached result stays fresh before the next CLI run kicks off a
/// new background check. The check is detached and its result is only read on
/// the *following* run, so there's no reason to hammer GitLab more than once a
/// day just to power a passive reminder.
const CHECK_INTERVAL_SECS: u64 = 60 * 60 * 24;

/// The argument `main` dispatches the hidden background-check subcommand on.
/// Kept here next to everything else that knows about the check so the two
/// ends stay in sync.
pub const CHECK_SUBCOMMAND: &str = "__check-updates";

/// Result of the last background check, cached so the reminder can be printed
/// without ever blocking a command on the network.
#[derive(Debug, Serialize, Deserialize)]
struct UpdateCheck {
    /// Unix seconds the check completed at — drives the `CHECK_INTERVAL_SECS`
    /// throttle.
    checked_at: u64,
    /// The latest tag reticulum's pipeline resolved to at that time.
    latest_tag: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn load_cache() -> Option<UpdateCheck> {
    let data = fs::read_to_string(CACHE_PATH).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_cache(check: &UpdateCheck) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(check)?;
    fs::write(CACHE_PATH, json)?;
    Ok(())
}

/// A cache older than `CHECK_INTERVAL_SECS` (or missing entirely) is due for a
/// refresh. A `checked_at` in the future (clock skew) counts as fresh.
fn is_stale(cache: &Option<UpdateCheck>) -> bool {
    match cache {
        None => true,
        Some(check) => now_secs().saturating_sub(check.checked_at) >= CHECK_INTERVAL_SECS,
    }
}

/// The monotonic pipeline id embedded in a resolved tag
/// (`0.0.0-experimental-<id>-<sha>`). Only these tags can be ordered; anything
/// else (a floating `latest`, a hand-pinned semver) returns `None`.
fn pipeline_id(tag: &str) -> Option<u64> {
    let rest = tag.strip_prefix("0.0.0-experimental-")?;
    let (id, _sha) = rest.split_once('-')?;
    id.parse().ok()
}

/// Whether `latest` is a strictly newer build than `current`. Both must be
/// resolvable pipeline tags to be comparable — a floating/unparseable current
/// tag (e.g. `latest`) is never "behind", since it re-pulls the newest image
/// on its own and there's nothing concrete to upgrade *to*.
fn is_newer(current: &str, latest: &str) -> bool {
    match (pipeline_id(current), pipeline_id(latest)) {
        (Some(current_id), Some(latest_id)) => latest_id > current_id,
        _ => false,
    }
}

/// Body of the hidden `__check-updates` subcommand: make the one network call
/// to resolve the latest build and cache it for the next run's reminder. Runs
/// detached, so this is the only place the resolve cost is paid, and entirely
/// best-effort — a failure just leaves the previous cache in place.
pub fn run_background_check() -> anyhow::Result<()> {
    let latest_tag = resolve_dev_tag()?;
    write_cache(&UpdateCheck {
        checked_at: now_secs(),
        latest_tag,
    })
}

/// Spawns `codezero __check-updates` as a detached child when there's a setup
/// to check against and the cache has gone stale. The parent returns
/// immediately without waiting, so the resolve happens off the user's critical
/// path; the child reparents to init once we exit. Entirely best-effort.
pub fn maybe_spawn_background_check() {
    if !Path::new(ENV_PATH).exists() || !is_stale(&load_cache()) {
        return;
    }

    let Ok(exe) = std::env::current_exe() else {
        return;
    };

    let _ = Command::new(exe)
        .arg(CHECK_SUBCOMMAND)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Prints a one-line reminder when the last cached check found a build newer
/// than what's currently pinned for either the core platform or actions.
/// No-op when there's no setup or no cache yet.
pub fn show_reminder() {
    let Some(cache) = load_cache() else {
        return;
    };

    let current_core = env_file::read_value(ENV_PATH, "IMAGE_TAG")
        .ok()
        .flatten()
        .unwrap_or_default();
    let current_actions = env_file::read_value(ENV_PATH, "ACTION_IMAGE_TAG")
        .ok()
        .flatten()
        .unwrap_or_default();

    if is_newer(&current_core, &cache.latest_tag) || is_newer(&current_actions, &cache.latest_tag) {
        println!();
        ui::muted_line(&format!(
            "A newer CodeZero build is available ({}).",
            cache.latest_tag
        ));
        ui::muted_line(
            "You can update now with `codezero --upgrade --dev` (experimental — a dedicated `upgrade` command is coming).",
        );
    }
}

/// Records a known-current tag so a just-finished `upgrade` clears any pending
/// reminder immediately, instead of waiting for the next background check.
pub fn record_current_tag(tag: &str) {
    let _ = write_cache(&UpdateCheck {
        checked_at: now_secs(),
        latest_tag: tag.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_id_parses_experimental_tags() {
        assert_eq!(
            pipeline_id("0.0.0-experimental-12345-abcdef0"),
            Some(12345)
        );
    }

    #[test]
    fn pipeline_id_rejects_non_experimental_tags() {
        assert_eq!(pipeline_id("latest"), None);
        assert_eq!(pipeline_id("1.2.3"), None);
        assert_eq!(pipeline_id("0.0.0-experimental-notanumber-sha"), None);
    }

    #[test]
    fn is_newer_compares_pipeline_ids() {
        let older = "0.0.0-experimental-100-aaa";
        let newer = "0.0.0-experimental-200-bbb";

        assert!(is_newer(older, newer));
        assert!(!is_newer(newer, older));
        assert!(!is_newer(newer, newer));
    }

    #[test]
    fn is_newer_is_false_for_floating_or_unparseable_current() {
        let latest = "0.0.0-experimental-200-bbb";

        assert!(!is_newer("latest", latest));
        assert!(!is_newer("1.2.3", latest));
        assert!(!is_newer("0.0.0-experimental-100-aaa", "latest"));
    }

    #[test]
    fn is_stale_when_missing_or_past_the_interval() {
        assert!(is_stale(&None));
        assert!(is_stale(&Some(UpdateCheck {
            checked_at: now_secs().saturating_sub(CHECK_INTERVAL_SECS + 1),
            latest_tag: "x".into(),
        })));
    }

    #[test]
    fn is_fresh_within_the_interval() {
        assert!(!is_stale(&Some(UpdateCheck {
            checked_at: now_secs(),
            latest_tag: "x".into(),
        })));
    }

    #[test]
    fn update_check_round_trips_through_json() {
        let check = UpdateCheck {
            checked_at: 1_700_000_000,
            latest_tag: "0.0.0-experimental-42-deadbee".into(),
        };

        let json = serde_json::to_string(&check).unwrap();
        let parsed: UpdateCheck = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.checked_at, check.checked_at);
        assert_eq!(parsed.latest_tag, check.latest_tag);
    }
}
