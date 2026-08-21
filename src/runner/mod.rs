mod docker;

pub use docker::DockerComposeRunner;

pub trait Runner {
    fn start(&self) -> anyhow::Result<()>;
    /// `wipe_volumes` also removes `postgres-data`/`generated-configs`
    /// (`down -v`) - only `reset` wants that; a plain `stop` should leave
    /// existing data/secrets intact so the next `start` resumes as-is.
    fn stop(&self, wipe_volumes: bool) -> anyhow::Result<()>;
    /// Forces `config-generator` (a one-shot `restart: "no"` container) to
    /// rerun on the next `up`/`start_service` call instead of Compose
    /// reusing its already-completed run - needed whenever `.env` changed
    /// (e.g. installing an action) since anything depending on
    /// `config-generator`'s output (like aquila) would otherwise start with
    /// stale generated config.
    fn regenerate_configs(&self) -> anyhow::Result<()>;
    /// Forces an already-running service's container process to actually
    /// restart. `start_service`'s `up -d` alone won't do this: Compose skips
    /// recreating a service whose own definition hasn't changed, even if a
    /// dependency (like `config-generator`) just reran and rewrote a shared
    /// volume it reads from - so after `regenerate_configs` +
    /// `start_service`, the dependent still needs an explicit `restart` to
    /// actually re-read the fresh file.
    fn restart(&self, service: &str) -> anyhow::Result<()>;
    /// Brings up a single service (e.g. a newly installed action), pulling
    /// its image first if it isn't present locally.
    fn start_service(&self, service: &str) -> anyhow::Result<()>;
    /// Stops and removes a single service's container (e.g. an uninstalled
    /// action). Must be called before its compose fragment file is deleted.
    fn stop_service(&self, service: &str) -> anyhow::Result<()>;
    /// Prints each service's current state (`docker compose ps`), streamed
    /// straight to the terminal so its native table formatting is preserved.
    fn status(&self) -> anyhow::Result<()>;
    /// Streams logs for one service, or every service when `None`, straight
    /// to the terminal so `--follow` behaves like a normal `tail -f`.
    fn logs(&self, service: Option<&str>, follow: bool, tail: Option<u32>) -> anyhow::Result<()>;
}

pub fn default_runner() -> DockerComposeRunner {
    DockerComposeRunner::default()
}

/// The standard "an action's aquila-visible config changed" dance, shared by
/// `install`/`uninstall`/`register`/`unregister`: `regenerate_configs` +
/// `start_service("aquila")` makes config-generator actually rerun (aquila's
/// `depends_on: service_completed_successfully` triggers it), then an
/// explicit `restart` is what makes the already-running aquila container
/// actually re-read the now-fresh config, since `start_service` alone leaves
/// an unchanged, already-running service untouched.
pub fn refresh_aquila_config(runner: &dyn Runner) -> anyhow::Result<()> {
    runner.regenerate_configs()?;
    runner.start_service("aquila")?;
    runner.restart("aquila")
}
