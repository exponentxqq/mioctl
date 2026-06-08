pub async fn run() -> Result<(), String> {
    crate::ui::app::run_tui().await
}
