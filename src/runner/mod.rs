mod docker;

pub use docker::DockerComposeRunner;

pub trait Runner {
    fn start(&self) -> anyhow::Result<()>;
    fn stop(&self) -> anyhow::Result<()>;
}

pub fn default_runner() -> DockerComposeRunner {
    DockerComposeRunner::default()
}
