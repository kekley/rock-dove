use std::{io::Read, path::Path};

use flate2::{Compression, bufread::ZlibEncoder};
#[cfg(target_os = "linux")]
const QUICKJS_BINARY: &str = "qjs-linux-x86_64";
#[cfg(target_os = "windows")]
const QUICKJS_BINARY: &str = "qjs-windows-x86_64.exe";
#[cfg(target_os = "macos")]
const QUICKJS_BINARY: &str = "qjs-darwin";

#[cfg(target_os = "linux")]
const YTDLP_BINARY: &str = "yt-dlp_linux";
#[cfg(target_os = "windows")]
const YTDLP_BINARY: &str = "yt-dlp.exe";
#[cfg(target_os = "macos")]
const YTDLP_BINARY: &str = "yt-dlp_macos";

fn download_latest_quickjs() -> Vec<u8> {
    const QUICK_JS_RELEASE_URL: &str =
        "https://github.com/quickjs-ng/quickjs/releases/latest/download/";
    let url = format!("{}{}", QUICK_JS_RELEASE_URL, QUICKJS_BINARY);
    let get = pollster::block_on(reqwest::get(url)).unwrap();
    let body = pollster::block_on(get.bytes()).unwrap();
    body.to_vec()
}
fn download_latest_ytdlp() -> Vec<u8> {
    const YTDLP_RELEASE_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/";
    let url = format!("{}{}", YTDLP_RELEASE_URL, YTDLP_BINARY);
    let get = pollster::block_on(reqwest::get(url)).unwrap();
    let body = pollster::block_on(get.bytes()).unwrap();
    body.to_vec()
}

#[tokio::main]
async fn main() {
    println!("cargo::rerun-if-changed=deps/{QUICKJS_BINARY}");
    println!("cargo::rerun-if-changed=deps/{YTDLP_BINARY}");

    let ytdlp_path = Path::new(DEPENDENCY_PATH).join(YTDLP_BINARY);

    if !ytdlp_path.exists() {
        let ytdlp = download_latest_ytdlp();

        let mut compression_buffer = Vec::with_capacity(4096);
        let mut encoder = ZlibEncoder::new(ytdlp.as_slice(), Compression::default());
        encoder.read_to_end(&mut compression_buffer).unwrap();
        std::fs::write(ytdlp_path, &compression_buffer).unwrap();
    }

    const DEPENDENCY_PATH: &str = "./deps/";
    let quickjs_path = Path::new(DEPENDENCY_PATH).join(QUICKJS_BINARY);

    if !quickjs_path.exists() {
        let quickjs = download_latest_quickjs();

        let mut compression_buffer = Vec::with_capacity(4096);
        let mut encoder = ZlibEncoder::new(quickjs.as_slice(), Compression::default());
        encoder.read_to_end(&mut compression_buffer).unwrap();
        std::fs::write(quickjs_path, &compression_buffer).unwrap();
    }
}
