use crate::runner::{Runner, default_runner};

pub fn logs(service: Option<String>, follow: bool, tail: Option<u32>) -> anyhow::Result<()> {
    default_runner().logs(service.as_deref(), follow, tail)
}
