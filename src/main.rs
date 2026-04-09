use std::{
    fs::{create_dir_all, metadata, set_permissions},
    io::Read,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use directories::ProjectDirs;
use rock_dove::{
    GuildContextKey, HTTPClientKey, QUICKJS_BINARY, YTDLP_BINARY,
    args::Args,
    bot::{MusicBot, guild_context::GuildContext},
    yt_dlp::{
        YtDlpKey,
        binaries::{BUNDLED_QUICKJS, BUNDLED_YTDLP},
        sidecar::YtDlpSidecar,
    },
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

    let bin_dir = get_bin_dir();
    let ytdlp_path = bin_dir.join(YTDLP_BINARY!());
    let quickjs_path = bin_dir.join(QUICKJS_BINARY!());

    ensure_ytdlp(&ytdlp_path);
    ensure_quickjs(&quickjs_path);

    let mut client = Client::builder(&token, intents)
        .type_map_insert::<YtDlpKey>(Arc::new(YtDlpSidecar::new(
            &ytdlp_path,
            &quickjs_path,
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

fn ensure_ytdlp(path: &Path) {
    if path.exists() {
        if path.is_dir() {
            panic!("Found a folder in the provided ytdlp path");
        } else {
            let mut perms = metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            set_permissions(path, perms).unwrap();

            return;
        }
    }
    let mut out_buffer = Vec::with_capacity(4096);
    let mut decoder = flate2::bufread::ZlibDecoder::new(BUNDLED_YTDLP);
    decoder.read_to_end(&mut out_buffer).unwrap();
    std::fs::write(path, out_buffer).unwrap();
    let mut perms = metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    set_permissions(path, perms).unwrap();
}
fn ensure_quickjs(path: &Path) {
    if path.exists() {
        if path.is_dir() {
            panic!("Found a folder in the provided quickjs path");
        } else {
            let mut perms = metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            set_permissions(path, perms).unwrap();
            return;
        }
    }
    let mut out_buffer = Vec::with_capacity(4096);
    let mut decoder = flate2::bufread::ZlibDecoder::new(BUNDLED_QUICKJS);
    decoder.read_to_end(&mut out_buffer).unwrap();
    std::fs::write(path, out_buffer).unwrap();
    let mut perms = metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    set_permissions(path, perms).unwrap();
}

fn get_bin_dir() -> PathBuf {
    let proj_dirs = ProjectDirs::from("kekley", "smekley", "bear_cove")
        .expect("Could not determine project directories");

    let dir = proj_dirs.cache_dir().join("bin");

    create_dir_all(&dir).expect("Failed to create bin directory");
    dir
}
