use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;

mod action;
mod bundle;
mod command;
mod env_file;
mod preflight;
mod runner;
mod template;
mod ui;

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
        /// Where to load the setup questions from (defaults to fetching the bundle live from reticulum)
        #[arg(long)]
        bundle: Option<PathBuf>,
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
    /// Change your existing setup (admin account, profiles, image info, ...) without resetting.
    Configure {
        /// Where to load the setup questions from (defaults to fetching the bundle live from reticulum)
        #[arg(long)]
        bundle: Option<PathBuf>,
    },
    /// List, install, and remove actions ("plugins").
    #[command(subcommand)]
    Plugin(PluginCommands),
    /// Show the status of CodeZero's services.
    Status,
    /// Stream logs from CodeZero (or a single service).
    Logs {
        /// Which service to show logs for (omit for every service)
        service: Option<String>,
        /// Keep streaming new log lines instead of exiting after the current output
        #[arg(short, long)]
        follow: bool,
        /// Only show the last N lines per service
        #[arg(long)]
        tail: Option<u32>,
    },
    /// Bump CodeZero's image version in place, without resetting your setup.
    Upgrade {
        /// Explicit version tag to switch to (prompts for one if omitted)
        #[arg(long)]
        tag: Option<String>,
        /// Explicit image registry to switch to
        #[arg(long)]
        registry: Option<String>,
        /// Explicit image edition to switch to (e.g. ce, cc)
        #[arg(long)]
        edition: Option<String>,
        /// For CodeZero developers: skip the prompt and grab the newest in-progress build automatically
        #[arg(long)]
        dev: bool,
    },
}

#[derive(Subcommand)]
enum PluginCommands {
    /// List available actions (and which ones you've already installed).
    Ls {
        /// Where to look up available actions (defaults to the catalog built into this binary)
        #[arg(long)]
        index: Option<PathBuf>,
    },
    /// Add an action to your CodeZero setup, e.g. `codezero plugin install gls-action`.
    Install {
        /// Which action to install, e.g. `gls-action`, or `gls-action@1.2.3` for a specific version
        name: String,
        /// Where to look up available actions (defaults to the catalog built into this binary)
        #[arg(long)]
        index: Option<PathBuf>,
    },
    /// Remove a previously installed action.
    Uninstall {
        /// Which action to remove (the same name you used with `install`)
        name: String,
        /// Where to look up available actions (defaults to the catalog built into this binary)
        #[arg(long)]
        index: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    preflight::check()?;

    match cli.command {
        Commands::Setup { bundle, dev } => command::setup::setup(bundle, dev),
        Commands::Start => command::start::start(),
        Commands::Stop => command::stop::stop(),
        Commands::Reset => command::reset::reset(),
        Commands::Configure { bundle } => command::configure::configure(bundle),
        Commands::Plugin(plugin_command) => match plugin_command {
            PluginCommands::Ls { index } => command::plugins::plugins(index),
            PluginCommands::Install { name, index } => command::install::install(index, name),
            PluginCommands::Uninstall { name, index } => command::uninstall::uninstall(index, name),
        },
        Commands::Status => command::status::status(),
        Commands::Logs { service, follow, tail } => command::logs::logs(service, follow, tail),
        Commands::Upgrade {
            tag,
            registry,
            edition,
            dev,
        } => command::upgrade::upgrade(tag, registry, edition, dev),
    }
}
