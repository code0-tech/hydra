//! The setup bundle and action catalog, baked into the binary so an installed
//! `codezero` (via `cargo install`, a downloaded release binary, or Homebrew)
//! works standalone, without needing this repo checked out nearby. `--bundle`/
//! `--index` still read from disk when explicitly passed, for local development
//! against an edited bundle.

pub const MANIFEST_JSON: &str = include_str!("../bundle/manifest.json");
pub const ACTIONS_INDEX_JSON: &str = include_str!("../actions/index.json");

const ENV_TERA: &str = include_str!("../bundle/env.tera");
const DOCKER_COMPOSE_YML: &str = include_str!("../bundle/docker-compose.yml");
const SERVICE_CONFIGURATION_JSON_TERA: &str = include_str!("../bundle/service.configuration.json.tera");

/// Looks up an embedded template by the same filename `bundle/manifest.json`
/// refers to it by.
pub fn template(name: &str) -> Option<&'static str> {
    match name {
        "env.tera" => Some(ENV_TERA),
        "docker-compose.yml" => Some(DOCKER_COMPOSE_YML),
        "service.configuration.json.tera" => Some(SERVICE_CONFIGURATION_JSON_TERA),
        _ => None,
    }
}
