use std::error::Error;

mod bot;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let token = std::env::var("DISCORD_OAUTH_TOKEN=")
        .map_err(|e| format!("Failed to get DISCORD_OAUTH_TOKEN=: {}", e))?;
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let web_url = std::env::var("WEB_URL")
        .unwrap_or_else(|_| format!("http://localhost:{}", port).to_string());

    bot::start(token, web_url).await?;

    Ok(())
}
