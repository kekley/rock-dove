use std::sync::Arc;

use bear_cove::{
    GuildContextKey, HTTPClientKey,
    bot::MusicBot,
    yt_dlp::{YtDlpKey, sidecar::YtDlpSidecar},
};
use serenity::{Client, all::GatewayIntents};
use songbird::SerenityInit;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let token = std::env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .type_map_insert::<YtDlpKey>(Arc::new(YtDlpSidecar::new("./binaries/yt-dlp_linux")))
        .type_map_insert::<HTTPClientKey>(reqwest::Client::new())
        .type_map_insert::<GuildContextKey>(Default::default())
        .register_songbird()
        .event_handler(MusicBot::default())
        .await
        .expect("Could not create client");

    tokio::spawn(async move {
        let _ = client
            .start()
            .await
            .map_err(|why| eprintln!("Client ended: {:?}", why));
    });
    tokio::signal::ctrl_c().await.unwrap();
    println!("bye");
}
