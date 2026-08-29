use std::error::Error;

use myurl_server::{AppConfig, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    run(AppConfig::from_process_env()?).await?;
    Ok(())
}
