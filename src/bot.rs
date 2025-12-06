use serenity::async_trait;
use serenity::builder::{
    CreateCommand, CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage,
};
use serenity::model::application::{CommandOptionType, Interaction};
use serenity::model::prelude::*;
use serenity::prelude::*;
use serde::Deserialize;
use tracing::{debug, error, info};

use crate::db::{CommandLog, Database};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PlayerInfo {
    count: u32,
    updated_at: String,
    age: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ServerInfo {
    name: String,
    description: String,
    #[serde(rename = "type")]
    server_type: String,
    software: String,
    host: String,
    port: String,
    website_url: Option<String>,
    discord_url: Option<String>,
    players: Option<PlayerInfo>,
}

async fn fetch_servers() -> Result<Vec<ServerInfo>, String> {
    let response = reqwest::get("https://treestats.net/servers.json")
        .await
        .map_err(|e| format!("Failed to fetch servers: {}", e))?;

    let servers: Vec<ServerInfo> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse servers: {}", e))?;

    Ok(servers)
}

fn find_server<'a>(servers: &'a [ServerInfo], query: &str) -> Option<&'a ServerInfo> {
    // First try exact case-insensitive match
    if let Some(server) = servers.iter().find(|s| s.name.eq_ignore_ascii_case(query)) {
        return Some(server);
    }

    // Fall back to fuzzy matching with a threshold
    // Require query to be at least 50% of the server name length to avoid very short queries matching
    const SIMILARITY_THRESHOLD: f64 = 0.8;
    const MIN_QUERY_LENGTH_RATIO: f64 = 0.5;

    servers
        .iter()
        .filter(|s| {
            let min_length = (s.name.len() as f64 * MIN_QUERY_LENGTH_RATIO).ceil() as usize;
            query.len() >= min_length
        })
        .map(|s| {
            let similarity = strsim::jaro_winkler(&s.name.to_lowercase(), &query.to_lowercase());
            (s, similarity)
        })
        .filter(|(_, similarity)| *similarity >= SIMILARITY_THRESHOLD)
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(server, _)| server)
}

pub struct Handler {
    pub web_url: String,
    pub db: Database,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Bot connected as: {}", ready.user.name);

        let http = &ctx.http;

        let status_command = CreateCommand::new("status").description("Check bot status");

        let server_command = CreateCommand::new("server")
            .description("Get connection info for an AC server")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "name",
                    "Server name (supports fuzzy matching)",
                )
                .required(true),
            );

        if let Err(e) = http.create_global_command(&status_command).await {
            error!("Failed to create status command: {}", e);
        }

        if let Err(e) = http.create_global_command(&server_command).await {
            error!("Failed to create server command: {}", e);
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            info!(
                "Received command {} from user {}",
                command.data.name, command.user.id
            );

            let content = match command.data.name.as_str() {
                "status" => "Okay".to_string(),
                "server" => {
                    let server_name = command
                        .data
                        .options
                        .iter()
                        .find(|opt| opt.name == "name")
                        .and_then(|opt| opt.value.as_str())
                        .unwrap_or("");

                    match fetch_servers().await {
                        Ok(servers) => {
                            if let Some(server) = find_server(&servers, server_name) {
                                let mut response = format!(
                                    "You can connect to {} at `{}:{}`.",
                                    server.name,
                                    server.host,
                                    server.port
                                );

                                match (&server.discord_url, &server.players) {
                                    (Some(discord_url), Some(players)) => {
                                        response.push_str(&format!(
                                            " {}'s Discord is {}. As of {}, {} character{} {} in the game world.",
                                            server.name,
                                            discord_url,
                                            players.age,
                                            players.count,
                                            if players.count == 1 { "" } else { "s" },
                                            if players.count == 1 { "was" } else { "were" }
                                        ));
                                    }
                                    (None, Some(players)) => {
                                        response.push_str(&format!(
                                            " {} doesn't have a Discord. As of {}, {} character{} {} in the game world.",
                                            server.name,
                                            players.age,
                                            players.count,
                                            if players.count == 1 { "" } else { "s" },
                                            if players.count == 1 { "was" } else { "were" }
                                        ));
                                    }
                                    (Some(discord_url), None) => {
                                        response.push_str(&format!(
                                            " {}'s Discord is {}. I don't seem to have any information on player counts. They must not use TreeStats :(",
                                            server.name,
                                            discord_url
                                        ));
                                    }
                                    (None, None) => {
                                        response.push_str(&format!(
                                            " {} doesn't have a Discord and I don't seem to have any information on player counts. They must not use TreeStats :(",
                                            server.name
                                        ));
                                    }
                                }

                                response
                            } else {
                                format!("Server '{}' not found. Please check the name and try again.", server_name)
                            }
                        }
                        Err(e) => {
                            error!("Failed to fetch servers: {}", e);
                            "Failed to fetch server list. Please try again later.".to_string()
                        }
                    }
                }
                _ => "Unknown command".to_string(),
            };

            let data = CreateInteractionResponseMessage::new().content(content);
            let builder = CreateInteractionResponse::Message(data);

            if let Err(e) = command.create_response(&ctx.http, builder).await {
                error!("Failed to respond to command: {}", e);
            }
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots (including ourselves)
        if msg.author.bot {
            return;
        }

        debug!(
            "Message received in {}: {} ({} attachments)",
            msg.channel_id,
            msg.content,
            msg.attachments.len()
        );

        // Check for PCAP attachments
        let pcap_attachment = msg
            .attachments
            .iter()
            .find(|a| a.filename.to_lowercase().contains(".pcap"));

        if let Some(attachment) = pcap_attachment {
            info!(
                "PCAP attachment detected: {} in channel {} message {}",
                attachment.filename, msg.channel_id, msg.id
            );

            let web_link = format!("{}?channel={}&msg={}", self.web_url, msg.channel_id, msg.id);
            let reply = format!("You can view your PCAP [here]({web_link})");

            let success = if let Err(e) = msg.reply(&ctx.http, reply).await {
                error!("Failed to send reply: {}", e);
                false
            } else {
                true
            };

            // Log command to database
            let log = CommandLog {
                command_name: "pcap_detect".to_string(),
                user_id: msg.author.id.to_string(),
                user_name: msg.author.name.clone(),
                channel_id: msg.channel_id.to_string(),
                guild_id: msg.guild_id.map(|id| id.to_string()),
                message_id: msg.id.to_string(),
                success,
                error_message: if success {
                    None
                } else {
                    Some("Failed to send reply".to_string())
                },
            };

            if let Err(e) = self.db.log_command(log).await {
                error!("Failed to log command to database: {}", e);
            }
        }
    }
}

/// Start the Discord bot
pub async fn start(
    token: String,
    web_url: String,
    db: Database,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting bot with WEB_URL={}", web_url);

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let handler = Handler { web_url, db };
    let mut client = Client::builder(&token, intents)
        .event_handler(handler)
        .await?;

    client.start().await?;

    Ok(())
}
