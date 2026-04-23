use crate::error::CliError;
use crate::output;

pub fn run() -> Result<(), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::user(format!("failed to get current directory: {e}")))?;

    match tc_storage::init::init_project(&cwd) {
        Ok(store) => {
            output::print_success(&format!(
                "Initialized tc project at {}",
                store.tc_dir().display()
            ));
            Ok(())
        }
        Err(tc_storage::StorageError::AlreadyInitialized(path)) => {
            output::print_warning(&format!(
                "Project already initialized at {}",
                path.join(".tc").display()
            ));
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
