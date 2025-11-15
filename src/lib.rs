use serenity::prelude::TypeMapKey;

pub mod bot;
pub mod git;
pub mod yt_dlp;

pub const SITE_PATH: &str = "/home/jesus/bear_cove/.venv/lib/python3.13/site-packages";

pub struct HTTPClientKey;

impl TypeMapKey for HTTPClientKey {
    type Value = reqwest::Client;
}
