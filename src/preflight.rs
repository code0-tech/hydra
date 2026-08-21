use std::process::{Command, Stdio};

fn succeeds(command: &mut Command) -> bool {
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Confirms Docker (and the Compose v2 plugin) are installed and the daemon
/// is reachable, so a missing prerequisite fails with one clear message up
/// front instead of a raw `docker: command not found` (or a stalled `up -d`)
/// surfacing from deep inside whichever subcommand happens to shell out first.
pub fn check() -> anyhow::Result<()> {
    if !succeeds(Command::new("docker").arg("--version")) {
        anyhow::bail!(
            "Docker isn't installed (or isn't on PATH).\nInstall it from https://docs.docker.com/get-docker/ and try again."
        );
    }

    if !succeeds(Command::new("docker").args(["compose", "version"])) {
        anyhow::bail!(
            "The Docker Compose v2 plugin isn't available.\nUpdate Docker to a version that bundles `docker compose`: https://docs.docker.com/compose/install/"
        );
    }

    if !succeeds(Command::new("docker").arg("info")) {
        anyhow::bail!(
            "Docker isn't running.\nStart Docker Desktop (or the Docker daemon) and try again."
        );
    }

    Ok(())
}
