#[cfg(target_os = "linux")]
macro_rules! YTDLP_BINARY {
    () => {
        "yt-dlp_linux"
    };
}

#[cfg(target_os = "windows")]
macro_rules! YTDLP_BINARY {
    () => {
        "yt-dlp.exe"
    };
}

#[cfg(target_os = "macos")]
macro_rules! YTDLP_BINARY {
    () => {
        "yt-dlp_macos"
    };
}

#[cfg(target_os = "linux")]
macro_rules! QUICKJS_BINARY {
    () => {
        "qjs-linux-x86_64"
    };
}

#[cfg(target_os = "windows")]
macro_rules! QUICKJS_BINARY {
    () => {
        "qjs-windows-x86_64.exe"
    };
}

#[cfg(target_os = "macos")]
macro_rules! QUICKJS_BINARY {
    () => {
        "qjs-darwin"
    };
}

use std::{io::Read, path::PathBuf};

use flate2::{Compression, bufread::ZlibEncoder};

fn download_latest_quickjs() -> Vec<u8> {
    const QUICKJS_RELEASE_URL: &str =
        "https://github.com/quickjs-ng/quickjs/releases/latest/download/";
    let url = format!("{}{}", QUICKJS_RELEASE_URL, QUICKJS_BINARY!());
    let get = pollster::block_on(reqwest::get(url)).unwrap();
    let body = pollster::block_on(get.bytes()).unwrap();
    body.to_vec()
}
fn download_latest_ytdlp() -> Vec<u8> {
    const YTDLP_RELEASE_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/";
    let url = format!("{}{}", YTDLP_RELEASE_URL, YTDLP_BINARY!());
    let get = pollster::block_on(reqwest::get(url)).unwrap();
    let body = pollster::block_on(get.bytes()).unwrap();
    body.to_vec()
}

#[tokio::main]
async fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ytdlp_path = PathBuf::from(&out_dir).join(YTDLP_BINARY!());
    let quickjs_path = PathBuf::from(&out_dir).join(QUICKJS_BINARY!());

    println!("{ytdlp_path:?}");
    println!("{quickjs_path:?}");

    if !ytdlp_path.exists() {
        let ytdlp = download_latest_ytdlp();

        let mut compression_buffer = Vec::with_capacity(4096);
        let mut encoder = ZlibEncoder::new(ytdlp.as_slice(), Compression::default());
        encoder.read_to_end(&mut compression_buffer).unwrap();
        std::fs::write(ytdlp_path, &compression_buffer).unwrap();
    }

    if !quickjs_path.exists() {
        let quickjs = download_latest_quickjs();

        let mut compression_buffer = Vec::with_capacity(4096);
        let mut encoder = ZlibEncoder::new(quickjs.as_slice(), Compression::default());
        encoder.read_to_end(&mut compression_buffer).unwrap();
        std::fs::write(quickjs_path, &compression_buffer).unwrap();
    }
}
