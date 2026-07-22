use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;

mod action;
mod bundle;
mod command;
mod runner;
mod template;
mod ui;

const DEFAULT_ACTION_INDEX: &str = "actions/index.json";

#[derive(Parser)]
#[command(name = "codezero")]
#[command(version = "0.1.0")]
#[command(about = "Get CodeZero running on your machine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up CodeZero: answer a few quick questions and we'll get everything running for you.
    Setup {
        /// Where to load the setup questions from (you usually don't need to change this)
        #[arg(long, default_value = "bundle")]
        bundle: PathBuf,
        /// For CodeZero developers: skip the questions and grab the newest in-progress build automatically
        #[arg(long)]
        dev: bool,
    },
    /// Start CodeZero back up.
    Start,
    /// Shut CodeZero down and free up the resources it was using.
    Stop,
    /// Start over: stops CodeZero, clears your local setup, and walks through setup again.
    Reset,
    /// Add an action to your CodeZero setup, e.g. `codezero install gls`.
    Install {
        /// Which action to install, e.g. `gls`, or `gls@1.2.3` for a specific version
        name: String,
        /// Where to look up available actions (you usually don't need to change this)
        #[arg(long, default_value = DEFAULT_ACTION_INDEX)]
        index: PathBuf,
    },
    /// Remove a previously installed action.
    Uninstall {
        /// Which action to remove (the same name you used with `install`)
        name: String,
        /// Where to look up available actions (you usually don't need to change this)
        #[arg(long, default_value = DEFAULT_ACTION_INDEX)]
        index: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Setup { bundle, dev } => command::setup::setup(bundle, dev),
        Commands::Start => command::start::start(),
        Commands::Stop => command::stop::stop(),
        Commands::Reset => command::reset::reset(),
        Commands::Install { name, index } => command::install::install(index, name),
        Commands::Uninstall { name, index } => command::uninstall::uninstall(index, name),
    }
}
