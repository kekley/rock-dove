use bear_cove::bot::MusicBot;
use serenity::{Client, all::GatewayIntents, prelude::TypeMapKey};
use songbird::SerenityInit;

struct ClientKey;
impl TypeMapKey for ClientKey {
    type Value = reqwest::Client;
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let token = std::env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .register_songbird()
        .event_handler(MusicBot::new())
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
