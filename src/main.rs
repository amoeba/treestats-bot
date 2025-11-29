use std::error::Error;

use log::info;

use crate::web::create_router;

mod bot;
mod discord;
mod web;

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {
            println!("\nReceived Ctrl+C, shutting down gracefully...");
        },
        _ = terminate => {
            println!("\nReceived termination signal, shutting down gracefully...");
        },
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let version = std::env::var("GIT_SHA_SHORT").unwrap_or_else(|_| "unknown".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{port}");
    let web_url = std::env::var("WEB_URL")
        .unwrap_or_else(|_| format!("http://localhost:{}", port).to_string());
    let token = std::env::var("DISCORD_OAUTH_TOKEN")
        .map_err(|e| format!("Failed to get DISCORD_OAUTH_TOKEN: {}", e))?;

    info!(
        "Starting bot process (sha={}) at {} with WEB_URL={}...",
        version, port, addr
    );
    tokio::spawn(async move {
        if let Err(e) = bot::start(token, web_url).await {
            log::error!("bot::start failed: {:?}", e);
        }
    });

    let app = create_router();
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind listener");

    info!("Web server running on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server failed");

    info!("Server shutdown complete");

    Ok(())
}
