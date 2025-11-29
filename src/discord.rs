//! Discord API integration for fetching message attachments

use axum::http::StatusCode;
use serde::Deserialize;
use tracing::{debug, error, warn};

#[derive(Debug, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
    #[allow(dead_code)]
    pub channel_id: String,
    pub attachments: Vec<DiscordAttachment>,
}

#[derive(Debug, Deserialize)]
pub struct DiscordAttachment {
    #[allow(dead_code)]
    pub id: String,
    pub filename: String,
    pub url: String,
    #[allow(dead_code)]
    pub content_type: Option<String>,
    #[allow(dead_code)]
    pub size: Option<u32>,
}

const DISCORD_API_BASE: &str = "https://discord.com/api/v9";
const MAX_ATTACHMENT_SIZE: usize = 100 * 1024 * 1024; // 100 MB
const TOKEN_PREFIX: &str = "Bot "; // Bot token prefix (required by Discord API)

/// Validate a Discord snowflake ID (17-19 digits, numeric only)
pub fn is_valid_snowflake(id: &str) -> bool {
    !id.is_empty() && id.len() >= 17 && id.len() <= 19 && id.chars().all(|c| c.is_ascii_digit())
}

/// Validate that filename has .pcap or .pcapng extension
fn is_pcap_file(filename: &str) -> bool {
    filename.ends_with(".pcap") || filename.ends_with(".pcapng")
}

/// Fetch message details from Discord API
pub async fn fetch_message(
    channel_id: &str,
    message_id: &str,
    token: &str,
) -> Result<DiscordMessage, (StatusCode, String)> {
    // Validate snowflake IDs
    if !is_valid_snowflake(channel_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid channel ID format".to_string(),
        ));
    }
    if !is_valid_snowflake(message_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid message ID format".to_string(),
        ));
    }

    let url = format!("{DISCORD_API_BASE}/channels/{channel_id}/messages/{message_id}");

    debug!("Fetching Discord message from: {}", url);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("{TOKEN_PREFIX}{token}"))
        .send()
        .await
        .map_err(|e| {
            error!("Failed to fetch Discord message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to connect to Discord API".to_string(),
            )
        })?;

    if response.status().is_success() {
        let message = response.json::<DiscordMessage>().await.map_err(|e| {
            error!("Failed to parse Discord message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to parse Discord response".to_string(),
            )
        })?;

        debug!("Successfully fetched message from Discord: {}", message.id);

        // Validate that message has at least one PCAP attachment
        let has_pcap = message
            .attachments
            .iter()
            .any(|a| is_pcap_file(&a.filename));

        if !has_pcap {
            warn!("Message has no PCAP attachments");
            return Err((
                StatusCode::BAD_REQUEST,
                "Message has no PCAP attachments (.pcap or .pcapng)".to_string(),
            ));
        }

        Ok(message)
    } else {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        error!("Discord API error: {} - {}", status, body);

        match status {
            401 => Err((
                StatusCode::UNAUTHORIZED,
                "Discord authentication failed (invalid or missing token)".to_string(),
            )),
            404 => Err((
                StatusCode::NOT_FOUND,
                "Discord message not found".to_string(),
            )),
            403 => Err((
                StatusCode::FORBIDDEN,
                "Access denied to Discord message".to_string(),
            )),
            _ => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Discord API error".to_string(),
            )),
        }
    }
}

/// Download attachment from URL
pub async fn download_attachment(url: &str) -> Result<Vec<u8>, (StatusCode, String)> {
    debug!("Downloading attachment from: {}", url);

    let client = reqwest::Client::new();
    let response = client.get(url).send().await.map_err(|e| {
        error!("Failed to download attachment: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to download attachment".to_string(),
        )
    })?;

    if response.status().is_success() {
        let content_length = response.content_length().unwrap_or(0) as usize;

        if content_length > MAX_ATTACHMENT_SIZE {
            warn!("Attachment too large: {} bytes", content_length);
            return Err((
                StatusCode::BAD_REQUEST,
                "Attachment exceeds maximum size limit (100 MB)".to_string(),
            ));
        }

        let bytes = response.bytes().await.map_err(|e| {
            error!("Failed to read attachment: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read attachment".to_string(),
            )
        })?;

        Ok(bytes.to_vec())
    } else {
        error!("Attachment download error: {}", response.status());
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to download attachment".to_string(),
        ))
    }
}
