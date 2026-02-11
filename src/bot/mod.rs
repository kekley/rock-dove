pub mod command;
pub mod guild_context;
pub mod queue;
pub mod track_notifier;
pub mod tracks;
pub mod undo_stack;
pub mod work_queue;

use serenity::{
    all::{ChannelId, Context, Message},
    async_trait,
};

use tracing::{Level, event};

use crate::bot::command::PreparedCommand;

#[derive(Debug, Default)]
pub struct MusicBot {}

#[async_trait]
impl serenity::all::EventHandler for MusicBot {
    async fn message(&self, ctx: Context, user_message: Message) {
        let reply_channel = user_message.channel_id;
        let _handle = match PreparedCommand::parse(user_message, &ctx).await {
            Ok(command) => tokio::spawn(command.execute(ctx.clone())),
            Err(err) => {
                event!(Level::INFO, "{err}");
                if let Some(reply) = err.to_reply() {
                    let _ = send_message(&ctx, reply_channel, &reply).await;
                }

                return;
            }
        };
    }
}

pub async fn send_message(ctx: &Context, channel: ChannelId, message: &str) -> Option<Message> {
    #[cfg(feature = "tracing")]
    event!(Level::INFO, "Sending chat message: {message}");

    match channel.say(&ctx.http, message).await {
        Ok(message) => Some(message),
        Err(err) => {
            #[cfg(feature = "tracing")]
            event!(Level::ERROR, "Error sending message: {err}");
            None
        }
    }
}
