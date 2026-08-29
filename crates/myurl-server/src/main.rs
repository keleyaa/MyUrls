use std::{env, error::Error};

use myurl_server::{AppConfig, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let port = env::var("APP_PORT")
        .ok()
        .map(|value| value.parse::<u16>())
        .transpose()?
        .unwrap_or(3000);

    run(AppConfig { port }).await?;
    Ok(())
}
