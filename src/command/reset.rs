use std::fs;

use console::style;

use crate::command::setup;

pub fn reset() -> anyhow::Result<()> {
    if fs::exists(".codezero")? {
        fs::remove_dir_all(".codezero")?;
        println!(
            "{} {}",
            style("Reset:").yellow().bold(),
            "removed existing .codezero configuration."
        );
    }

    setup::setup()
}
