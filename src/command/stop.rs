use crate::runner::{Runner, default_runner};

pub fn stop() -> anyhow::Result<()> {
    default_runner().stop()
}
