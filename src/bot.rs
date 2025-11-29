use serenity::async_trait;
use serenity::model::prelude::*;
use serenity::prelude::*;
use tracing::{debug, error, info};

pub struct Handler {
    pub web_url: String,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Bot connected as: {}", ready.user.name);
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
            let reply = format!("You can view your PCAP [here]({})", web_link);

            if let Err(e) = msg.reply(&ctx.http, reply).await {
                error!("Failed to send reply: {}", e);
            }
        }
    }
}

/// Start the Discord bot
pub async fn start(token: String, web_url: String) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting bot with WEB_URL={}", web_url);

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let handler = Handler { web_url };
    let mut client = Client::builder(&token, intents)
        .event_handler(handler)
        .await?;

    client.start().await?;

    Ok(())
}
