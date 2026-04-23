mod cli;
mod commands;
mod error;
mod output;

use clap::Parser;

use cli::{Cli, Commands};
use error::CliError;

fn main() -> miette::Result<()> {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .context_lines(3)
                .build(),
        )
    }))
    .ok();

    let cli = Cli::parse();

    match cli.command {
        Some(cmd) => run_command(cmd),
        None => commands::tui::run().map_err(Into::into),
    }
    .map_err(Into::into)
}

fn run_command(cmd: Commands) -> Result<(), CliError> {
    match cmd {
        Commands::Init => commands::init::run(),
        Commands::Add(args) => commands::add::run(args),
        Commands::List(args) => commands::list::run(args),
        Commands::Show { id } => commands::show::run(&id),
        Commands::Edit { id } => commands::edit::run(&id),
        Commands::Delete(args) => commands::delete::run(args),
        Commands::Done { id } => commands::status::run_done(&id),
        Commands::Block { id, reason } => commands::status::run_block(&id, &reason),
        Commands::Status { id, status } => commands::status::run_set(&id, &status),
        Commands::Next => commands::next::run(),
        Commands::Validate => commands::validate::run(),
        Commands::Stats => commands::stats::run(),
        Commands::Graph(args) => commands::graph::run(args),
        Commands::Pack(args) => commands::pack::run(args),
        Commands::Plan(args) => commands::plan::run(args),
        Commands::Impl(args) => commands::impl_::run(args),
        Commands::Spawn(args) => commands::spawn::run(args),
        Commands::Workers(args) => commands::spawn::run_workers(args),
        Commands::Logs(args) => commands::spawn::run_logs(args),
        Commands::Kill(args) => commands::spawn::run_kill(args),
        Commands::Attach(args) => commands::spawn::run_attach(args),
        Commands::Review(args) => commands::review::run(args),
        Commands::Merge(args) => commands::review::run_merge(args),
        Commands::Test(args) => commands::test::run(args),
        Commands::Epic(args) => commands::epic::run(args),
        Commands::Import(args) => commands::import::run(args),
        Commands::Config(args) => commands::config::run(args),
        Commands::Changelog(args) => commands::changelog::run(args),
        Commands::Tui => commands::tui::run().map_err(Into::into),
        Commands::Completion { shell } => commands::completion::run(shell),
    }
}
