use crate::cli::MigrateArgs;
use crate::error::CliError;
use crate::output;

/// Exit code returned when `--check` finds the file would change. Mirrors
/// what users expect from `--check`-style flags in formatters/linters.
const CHECK_FAILED_EXIT: i32 = 1;

pub fn run(args: MigrateArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;

    if args.check {
        let (report, _) = store.plan_migration()?;
        if report.changed {
            output::print_warning(&format!(
                "tasks.yaml would be migrated ({} tasks, {} -> {} bytes); rerun without --check to apply",
                report.tasks_loaded, report.bytes_before, report.bytes_after,
            ));
            std::process::exit(CHECK_FAILED_EXIT);
        }
        output::print_success(&format!(
            "tasks.yaml is up to date ({} tasks)",
            report.tasks_loaded
        ));
        return Ok(());
    }

    if args.dry_run {
        let (report, normalized) = store.plan_migration()?;
        report_outcome(&report, "would write");
        if report.changed {
            println!("---\n{normalized}");
        }
        return Ok(());
    }

    let report = store.migrate_tasks()?;
    report_outcome(&report, "wrote");
    Ok(())
}

fn report_outcome(report: &tc_storage::MigrationReport, verb: &str) {
    if report.changed {
        output::print_success(&format!(
            "{verb} normalized tasks.yaml ({} tasks, {} -> {} bytes)",
            report.tasks_loaded, report.bytes_before, report.bytes_after,
        ));
    } else {
        output::print_success(&format!(
            "tasks.yaml already up to date ({} tasks)",
            report.tasks_loaded
        ));
    }
}
