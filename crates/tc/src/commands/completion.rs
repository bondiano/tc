use clap::CommandFactory;
use clap_complete::Shell;

use crate::cli::Cli;
use crate::error::CliError;

pub fn run(shell: Shell) -> Result<(), CliError> {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "tc", &mut std::io::stdout());
    Ok(())
}
