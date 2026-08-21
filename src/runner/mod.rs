mod docker;

pub use docker::DockerComposeRunner;

pub trait Runner {
    fn start(&self) -> anyhow::Result<()>;
    /// `wipe_volumes` also removes `postgres-data`/`generated-configs`
    /// (`down -v`) - only `reset` wants that; a plain `stop` should leave
    /// existing data/secrets intact so the next `start` resumes as-is.
    fn stop(&self, wipe_volumes: bool) -> anyhow::Result<()>;
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
