use crate::cli::PackArgs;
use crate::error::CliError;
use crate::output;

use tc_packer::{PackOptions, PackStyle};

pub fn run(args: PackArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let config = store.load_config()?;

    let mut include_paths = Vec::new();
    let mut exclude_patterns = config.packer.ignore_patterns.clone();

    // If task ID given, use task.files as include paths
    if let Some(ref task_id) = args.task_id {
        let tasks = store.load_tasks()?;
        let task = tasks.iter().find(|t| t.id.0 == *task_id).ok_or_else(|| {
            CliError::Core(tc_core::error::CoreError::TaskNotFound(task_id.clone()))
        })?;
        include_paths.extend(task.files.clone());
        exclude_patterns.extend(task.pack_exclude.clone());
    }

    // If epic given, collect files from all tasks in that epic
    if let Some(ref epic) = args.epic {
        let tasks = store.load_tasks()?;
        for task in &tasks {
            if task.epic == *epic {
                include_paths.extend(task.files.clone());
                exclude_patterns.extend(task.pack_exclude.clone());
            }
        }
    }

    let style = PackStyle::from(config.packer.style.as_str());

    let options = PackOptions {
        root: store.root().clone(),
        include_paths,
        exclude_patterns,
        token_budget: config.packer.token_budget,
        style,
    };

    // Estimate-only mode
    if args.estimate {
        let (tokens, file_count) = tc_packer::estimate(&options)?;
        println!("~{tokens} tokens ({file_count} files)");
        return Ok(());
    }

    let result = tc_packer::pack(&options)?;

    for warning in &result.warnings {
        output::print_warning(warning);
    }

    print!("{}", result.content);

    eprintln!(
        "\n--- {} files, ~{} tokens ---",
        result.file_count, result.token_estimate
    );

    Ok(())
}
