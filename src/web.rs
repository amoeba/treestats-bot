use axum::{
    Json, Router,
    extract::{Path, Request},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use http::{HeaderValue, Method, StatusCode};
use log::info;
use serde::{Deserialize, Serialize};
use tower_http::{cors::AllowOrigin, services::ServeDir, trace::TraceLayer};

use crate::discord::{download_attachment, fetch_message};

#[derive(Deserialize)]
struct DiscordParams {
    channel_id: String,
    message_id: String,
}

#[derive(Serialize)]
struct DiscordError {
    error: String,
}

async fn log_requests(req: Request<axum::body::Body>, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let res = next.run(req).await;
    let status = res.status();
    println!(">>> {method} {uri} {status}");

    res
}

async fn discord_pull(
    Path(params): Path<DiscordParams>,
) -> Result<Vec<u8>, (StatusCode, Json<DiscordError>)> {
    println!(
        "==> Discord pull request: channel={}, msg={}",
        params.channel_id, params.message_id
    );
    info!(
        "Discord pull request: channel={}, msg={}",
        params.channel_id, params.message_id
    );

    // Check if token is available
    let token = std::env::var("DISCORD_OAUTH_TOKEN").map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(DiscordError {
                error: "Discord OAuth token not configured".to_string(),
            }),
        )
    })?;

    // Fetch message from Discord API
    let message = fetch_message(&params.channel_id, &params.message_id, &token)
        .await
        .map_err(|(status, error)| (status, Json(DiscordError { error })))?;

    // Find first PCAP attachment
    let pcap_attachment = message
        .attachments
        .iter()
        .find(|a| a.filename.ends_with(".pcap") || a.filename.ends_with(".pcapng"))
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(DiscordError {
                    error: "No PCAP attachments found in message".to_string(),
                }),
            )
        })?;

    // Download the attachment
    let pcap_data = download_attachment(&pcap_attachment.url)
        .await
        .map_err(|(status, error)| (status, Json(DiscordError { error })))?;

    info!(
        "Successfully fetched PCAP from Discord: {} ({} bytes)",
        pcap_attachment.filename,
        pcap_data.len()
    );

    Ok(pcap_data)
}

async fn health() -> &'static str {
    info!("Health check endpoint called");
    "OK"
}

pub fn create_router() -> Router {
    let dist_path = std::path::PathBuf::from("dist");
    use tower_http::cors::{Any, CorsLayer};

    // CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_headers(Any)
        .expose_headers(Any);

    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/discord/channels/{channel_id}/messages/{message_id}/attachments",
            get(discord_pull),
        )
        .fallback_service(ServeDir::new(&dist_path))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(log_requests))
}
