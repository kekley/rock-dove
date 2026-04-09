use std::path::PathBuf;

#[derive(clap::Parser)]
pub struct Args {
    #[clap(short = 'c', long, default_value = "./cookies.txt")]
    pub cookies_path: PathBuf,
    #[clap(short = 'd', long)]
    pub discord_token: String,
    #[clap(short = 'p', long, default_value = "./persist.json")]
    pub persistance_path: PathBuf,
}
