use std::path::PathBuf;

#[cfg(target_os = "windows")]
#[derive(clap::Parser)]
pub struct Args {
    #[clap(short = 'y', long, default_value = "./yt-dlp.exe")]
    pub ytdlp_binary_path: PathBuf,
    #[clap(short = 'c', long, default_value = "./cookies.txt")]
    pub cookies_path: PathBuf,
    #[clap(short = 'd', long)]
    pub discord_token: String,
    #[clap(short = 'p', long, default_value = "./persist.json")]
    pub persistance_path: PathBuf,
}

#[cfg(target_os = "macos")]
#[derive(clap::Parser)]
pub struct Args {
    #[clap(short = 'y', long, default_value = "./yt-dlp_macos")]
    pub ytdlp_binary_path: PathBuf,
    #[clap(short = 'c', long, default_value = "./cookies.txt")]
    pub cookies_path: PathBuf,
    #[clap(short = 'd', long)]
    pub discord_token: String,
    #[clap(short = 'p', long, default_value = "./persist.json")]
    pub persistance_path: PathBuf,
}

#[cfg(target_os = "linux")]
#[derive(clap::Parser)]
pub struct Args {
    #[clap(short = 'y', long, default_value = "./yt-dlp_linux")]
    pub ytdlp_binary_path: PathBuf,
    #[clap(short = 'c', long, default_value = "./cookies.txt")]
    pub cookies_path: PathBuf,
    #[clap(short = 'd', long)]
    pub discord_token: String,
    #[clap(short = 'p', long, default_value = "./persist.json")]
    pub persistance_path: PathBuf,
}
