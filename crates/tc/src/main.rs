#[cfg(not(unix))]
compile_error!(
    "tc currently supports Unix only (macOS, Linux). Windows is not supported; PRs welcome."
);

mod cli;
mod cli_parsers;
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

    // Single Tokio runtime for the whole process. Every subcommand used to
    // spin up its own Runtime -- wasteful and meant a shared HTTP client or
    // task coordinator couldn't live for longer than one command.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| miette::miette!("failed to build tokio runtime: {e}"))?;

    let result = rt.block_on(async move {
        match cli.command {
            Some(cmd) => dispatch(cmd).await,
            None => commands::tui::run().map_err(Into::into),
        }
    });

    result.map_err(Into::into)
}

async fn dispatch(cmd: Commands) -> Result<(), CliError> {
    match cmd {
        Commands::Init => commands::init::run(),
        Commands::Add(args) => commands::add::run(args),
        Commands::List(args) => commands::list::run(args),
        Commands::Show { id } => commands::show::run(&id),
        Commands::Edit(args) => commands::edit::run(args),
        Commands::Delete(args) => commands::delete::run(args),
        Commands::Done { id } => commands::status::run_done(&id),
        Commands::Block { id, reason } => commands::status::run_block(&id, &reason),
        Commands::Status { id, status } => commands::status::run_set(&id, &status),
        Commands::Next => commands::next::run(),
        Commands::Today => commands::views::today(),
        Commands::Upcoming(args) => commands::views::upcoming(args.days),
        Commands::Inbox => commands::views::inbox(),
        Commands::Overdue => commands::views::overdue(),
        Commands::Find(args) => commands::find::run(args),
        Commands::Validate => commands::validate::run(),
        Commands::Stats => commands::stats::run(),
        Commands::Graph(args) => commands::graph::run(args),
        Commands::Pack(args) => commands::pack::run(args),
        Commands::Plan(args) => commands::plan::run(args).await,
        Commands::Impl(args) => commands::impl_::run(args).await,
        Commands::Spawn(args) => commands::spawn::run(args).await,
        Commands::Workers(args) => commands::spawn::run_workers(args),
        Commands::Logs(args) => commands::spawn::run_logs(args),
        Commands::Kill(args) => commands::spawn::run_kill(args),
        Commands::Attach(args) => commands::spawn::run_attach(args),
        Commands::Review(args) => commands::review::run(args),
        Commands::Merge(args) => commands::review::run_merge(args),
        Commands::Test(args) => commands::test::run(args).await,
        Commands::Epic(args) => commands::epic::run(args),
        Commands::Import(args) => commands::import::run(args).await,
        Commands::Export(args) => commands::export::run(args),
        Commands::Config(args) => commands::config::run(args),
        Commands::Changelog(args) => commands::changelog::run(args),
        Commands::Migrate(args) => commands::migrate::run(args),
        Commands::Tui => commands::tui::run().map_err(Into::into),
        Commands::Ui(args) => commands::ui::run(args),
        Commands::Completion { shell } => commands::completion::run(shell),
    }
}
