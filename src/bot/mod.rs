pub mod command;
pub mod guild_context;
pub mod util;

use serenity::{
    all::{Context, Message},
    async_trait,
};

use tracing::{Level, event};

use crate::bot::{command::parse::PreparedCommand, util::send_message};

#[derive(Debug, Default)]
pub struct MusicBot {}

#[async_trait]
impl serenity::all::EventHandler for MusicBot {
    async fn message(&self, ctx: Context, user_message: Message) {
        let reply_channel = user_message.channel_id;
        let _handle = match PreparedCommand::parse_discord_message(user_message, &ctx).await {
            Ok(command) => tokio::spawn(command.execute(ctx.clone())),
            Err(parse_error) => {
                if parse_error.should_log() {
                    event!(Level::INFO, "{parse_error}");
                }
                if let Some(reply) = parse_error.user_reply() {
                    let _ = send_message(&ctx, reply_channel, &reply).await;
                }

                return;
            }
        };
    }
}
