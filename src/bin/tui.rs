use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    illef_workflow::tui::run().await
}
