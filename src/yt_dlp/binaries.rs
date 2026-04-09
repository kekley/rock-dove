#[cfg(target_os = "linux")]
#[macro_export]
macro_rules! YTDLP_BINARY {
    () => {
        "yt-dlp_linux"
    };
}

#[cfg(target_os = "windows")]
#[macro_export]
macro_rules! YTDLP_BINARY {
    () => {
        "yt-dlp.exe"
    };
}

#[cfg(target_os = "macos")]
#[macro_export]
macro_rules! YTDLP_BINARY {
    () => {
        "yt-dlp_macos"
    };
}

#[cfg(target_os = "linux")]
#[macro_export]
macro_rules! QUICKJS_BINARY {
    () => {
        "qjs-linux-x86_64"
    };
}

#[cfg(target_os = "windows")]
#[macro_export]
macro_rules! QUICKJS_BINARY {
    () => {
        "qjs-windows-x86_64.exe"
    };
}

#[cfg(target_os = "macos")]
#[macro_export]
macro_rules! QUICKJS_BINARY {
    () => {
        "qjs-darwin"
    };
}

pub const BUNDLED_YTDLP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/", YTDLP_BINARY!()));
pub const BUNDLED_QUICKJS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/", QUICKJS_BINARY!()));
