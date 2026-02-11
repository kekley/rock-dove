use std::path::PathBuf;

#[derive(clap::Args)]
pub struct Args {
    ytdlp_binary_path: PathBuf,
    cookies_path: PathBuf,
    discord_token: String,
    ytdlp_release_url: String,
}
