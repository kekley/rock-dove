pub mod args;
use std::sync::Arc;

use serenity::{all::GuildId, prelude::TypeMapKey};
use tokio::sync::RwLock;

use crate::bot::guild_context::GuildContext;

pub mod bot;
pub mod git;
pub mod yt_dlp;

pub const SITE_PATH: &str = "/home/jesus/bear_cove/.venv/lib/python3.13/site-packages";

pub struct HTTPClientKey;

pub struct GuildContextKey;

impl TypeMapKey for HTTPClientKey {
    type Value = reqwest::Client;
}

type Guilds = Vec<(GuildId, Arc<RwLock<GuildContext>>)>;

impl TypeMapKey for GuildContextKey {
    type Value = Guilds;
}
