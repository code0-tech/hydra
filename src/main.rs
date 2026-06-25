use clap::Parser;
use clap::Subcommand;

mod command;
mod runner;
pub mod settings;
mod template;

#[derive(Parser)]
#[command(name = "codezero")]
#[command(version = "0.1.0")]
#[command(about = "CodeZero interactive setup wizard")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Setup {},
    Start,
    Stop,
    Reset,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Setup {} => command::setup::setup(),
        Commands::Start => command::start::start(),
        Commands::Stop => command::stop::stop(),
        Commands::Reset => command::reset::reset(),
    }
}
