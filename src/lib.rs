pub mod args;
use std::sync::Arc;

use serenity::{all::GuildId, prelude::TypeMapKey};
use tokio::sync::RwLock;

use crate::bot::guild_context::GuildContext;

pub mod bot;
pub mod yt_dlp;

pub struct HTTPClientKey;

pub struct GuildContextKey;

impl TypeMapKey for HTTPClientKey {
    type Value = reqwest::Client;
}

type Guilds = Vec<(GuildId, Arc<RwLock<GuildContext>>)>;

impl TypeMapKey for GuildContextKey {
    type Value = Guilds;
}
