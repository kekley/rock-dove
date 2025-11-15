use bear_cove::{
    HTTPClientKey, SITE_PATH,
    bot::MusicBot,
    yt_dlp::{YtDlpKey, sidecar::YtDlpSidecar},
};
use pyembed::{MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use serenity::{Client, all::GatewayIntents, prelude::TypeMapKey};
use songbird::SerenityInit;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let token = std::env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .type_map_insert::<YtDlpKey>(YtDlpSidecar::new("./binaries/yt-dlp_linux"))
        .type_map_insert::<HTTPClientKey>(reqwest::Client::new())
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

fn _a() {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.filesystem_importer = true;
    config.allocator_backend = pyembed::MemoryAllocatorBackend::Mimalloc;

    let interpreter = MainPythonInterpreter::new(config).unwrap();

    interpreter.with_gil(|py| {
        let sys = py.import("sys").unwrap();
        let path = sys.getattr("path").unwrap();
        path.call_method1("append", (SITE_PATH,)).unwrap();

        let yt_dlp = py.import("yt_dlp").unwrap();
    });
}
