use std::sync::Arc;

use clap::Parser;
use rock_dove::{
    GuildContextKey, HTTPClientKey,
    args::Args,
    bot::{MusicBot, guild_context::GuildContext},
    yt_dlp::{YtDlpKey, sidecar::YtDlpSidecar},
};
use serenity::{
    Client,
    all::{GatewayIntents, GuildId},
};
use songbird::SerenityInit;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let token = args.discord_token.clone();

    let guild_data: Vec<(GuildId, Arc<RwLock<GuildContext>>)> =
        if let Ok(file_contents) = std::fs::read_to_string(&args.persistance_path) {
            let guild_data: Vec<(GuildId, GuildContext)> =
                serde_json::from_str(&file_contents).unwrap_or_default();
            guild_data
                .into_iter()
                .map(|(guild, ctx)| (guild, Arc::new(RwLock::new(ctx))))
                .collect()
        } else {
            vec![]
        };

    tracing_subscriber::fmt::init();

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .type_map_insert::<YtDlpKey>(Arc::new(YtDlpSidecar::new(
            args.ytdlp_binary_path.as_path(),
            Some(args.cookies_path.as_path()),
        )))
        .type_map_insert::<HTTPClientKey>(reqwest::Client::new())
        .type_map_insert::<GuildContextKey>(guild_data)
        .register_songbird()
        .event_handler(MusicBot::default())
        .await
        .expect("Could not create client");
    let client_data = client.data.clone();

    tokio::spawn(async move {
        let _ = client
            .start()
            .await
            .map_err(|why| eprintln!("Client ended: {:?}", why));
    });
    tokio::signal::ctrl_c().await.unwrap();
    let read_guard = client_data.read().await;
    let guild_data = read_guard
        .get::<GuildContextKey>()
        .expect("Guild data was not initialized");
    let mut persist_data = Vec::with_capacity(guild_data.len());
    for (guild, ctx_lock) in guild_data.iter() {
        let cloned: GuildContext = ctx_lock.read().await.clone();
        persist_data.push((guild, cloned));
    }
    let serialized = serde_json::to_string_pretty(&persist_data).expect("Could not serialize data");
    std::fs::write(&args.persistance_path, serialized).expect("Could not write serialized data");
    println!("bye");
}
