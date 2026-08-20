use crate::runner::{Runner, default_runner};

pub fn status() -> anyhow::Result<()> {
    default_runner().status()
}
