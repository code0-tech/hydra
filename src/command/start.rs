use crate::runner::{Runner, default_runner};

pub fn start() -> anyhow::Result<()> {
    default_runner().start()
}
