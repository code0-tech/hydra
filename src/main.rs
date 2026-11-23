use std::path::PathBuf;

use clap::CommandFactory;
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
mod update_check;

#[derive(Parser)]
#[command(name = "codezero")]
#[command(version = "0.1.0")]
#[command(about = "Get CodeZero running on your machine")]
struct Cli {
    /// EXPERIMENTAL: hard-update CodeZero to the latest build in place, keeping your setup.
    ///
    /// This is a stopgap entry point surfaced by the "new version available"
    /// reminder — it's experimental and will be replaced by a dedicated
    /// `upgrade` command in the future. Pass `--dev` (or `--tag`) to pick the
    /// build to move to.
    #[arg(long)]
    upgrade: bool,

    /// With `--upgrade`: pin a specific version tag instead of resolving a build.
    #[arg(long)]
    tag: Option<String>,

    /// With `--upgrade`: explicit image registry to switch to.
    #[arg(long)]
    registry: Option<String>,

    /// With `--upgrade`: explicit image edition to switch to (e.g. ce, cc).
    #[arg(long)]
    edition: Option<String>,

    /// With `--upgrade`: where to load the bundle from (defaults to fetching it live from reticulum).
    #[arg(long)]
    bundle: Option<PathBuf>,

    /// With `--upgrade`: resolve and move to the newest in-progress reticulum build.
    #[arg(long)]
    dev: bool,

    #[command(subcommand)]
    command: Option<Commands>,
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
    /// Internal: resolve the latest build and cache it for the update reminder.
    #[command(hide = true, name = "__check-updates")]
    CheckUpdates,
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
    /// For developing a new action locally: register an identifier with Aquila
    /// without a catalog entry or managed container, so you can point your
    /// own in-progress action (running via `npm run dev`, `cargo run`, ...) at
    /// it instead of round-tripping through an image build/`plugin install`.
    Register {
        /// Whatever you want to call your in-progress action
        identifier: String,
    },
    /// Remove a previously `register`ed identifier.
    Unregister {
        /// The same identifier you used with `register`
        identifier: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // The detached background check runs before anything else: it needs no
    // Docker (so no preflight) and must not re-trigger the reminder/spawn
    // wiring below, which would recurse.
    if let Some(Commands::CheckUpdates) = cli.command {
        let _ = update_check::run_background_check();
        return Ok(());
    }

    preflight::check()?;

    // Experimental `--upgrade` entry point surfaced by the reminder. Handled
    // before the reminder/background wiring: it resolves and applies the
    // upgrade itself, so there's nothing left to remind about afterward.
    if cli.upgrade {
        return command::upgrade::upgrade(cli.tag, cli.registry, cli.edition, cli.bundle, cli.dev);
    }

    // Surface whatever the previous run's detached check found, then kick off a
    // fresh one for next time. Both are best-effort and never block a command.
    update_check::show_reminder();
    update_check::maybe_spawn_background_check();

    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };

    match command {
        Commands::Setup { bundle, dev } => command::setup::setup(bundle, dev),
        Commands::Start => command::start::start(),
        Commands::Stop => command::stop::stop(),
        Commands::Reset => command::reset::reset(),
        Commands::Configure { bundle } => command::configure::configure(bundle),
        Commands::Plugin(plugin_command) => match plugin_command {
            PluginCommands::Ls { index } => command::plugins::plugins(index),
            PluginCommands::Install { name, index } => command::install::install(index, name),
            PluginCommands::Uninstall { name, index } => command::uninstall::uninstall(index, name),
            PluginCommands::Register { identifier } => command::register::register(identifier),
            PluginCommands::Unregister { identifier } => command::register::unregister(identifier),
        },
        Commands::Status => command::status::status(),
        Commands::Logs {
            service,
            follow,
            tail,
        } => command::logs::logs(service, follow, tail),
        // Handled above, before preflight.
        Commands::CheckUpdates => Ok(()),
    }
}
