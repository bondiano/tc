use tc_core::theme::Theme;

use crate::cli::{UiAction, UiArgs, UiThemeArgs};
use crate::error::CliError;
use crate::output;

pub fn run(args: UiArgs) -> Result<(), CliError> {
    match args.action {
        UiAction::Theme(t) => run_theme(t),
    }
}

fn run_theme(args: UiThemeArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;

    let Some(name) = args.name else {
        let cfg = store.load_config()?;
        println!("{}", cfg.ui.theme);
        return Ok(());
    };

    if name == "list" {
        let cfg = store.load_config()?;
        for preset in Theme::PRESET_NAMES {
            let marker = if *preset == cfg.ui.theme { "*" } else { " " };
            println!("{marker} {preset}");
        }
        return Ok(());
    }

    if Theme::by_name(&name).is_none() {
        return Err(CliError::user(format!(
            "unknown theme '{name}' (valid: {})",
            Theme::PRESET_NAMES.join(", ")
        )));
    }

    let mut cfg = store.load_config()?;
    cfg.ui.theme = name.clone();
    cfg.validate()
        .map_err(|e| CliError::user(format!("invalid config after theme change: {e}")))?;
    store.save_config(&cfg)?;

    output::print_success(&format!("theme set to '{name}'"));
    Ok(())
}
