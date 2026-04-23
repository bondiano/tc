use tc_tui::error::TuiError;

pub fn run() -> Result<(), TuiError> {
    tc_tui::runtime::run()
}
