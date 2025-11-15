use serenity::prelude::TypeMapKey;
use thiserror::Error;

use crate::yt_dlp::{playlist::VideoInfo, sidecar::YtDlpSidecar, video::VideoStreamInfo};

pub mod args;
pub mod format;
pub mod playlist;
pub mod py_runtime;
pub mod sidecar;
pub mod thumbnail;
pub mod video;

pub struct YtDlpKey;

impl TypeMapKey for YtDlpKey {
    type Value = YtDlpSidecar;
}

#[derive(Debug, Clone, Copy)]
pub enum VideoQuery<'a> {
    Url(&'a str),
    SearchTerm(&'a str),
}

impl<'a> VideoQuery<'a> {
    pub fn new_from_str(str: &'a str) -> Self {
        if str.trim().starts_with("https://") {
            VideoQuery::Url(str)
        } else {
            VideoQuery::SearchTerm(str)
        }
    }
    pub fn is_playlist(&self) -> bool {
        match self {
            VideoQuery::Url(url) => url.contains("?list="),
            VideoQuery::SearchTerm(_) => false,
        }
    }
}

#[derive(Error, Debug)]
pub enum YtDlpError {
    #[error("Json Parse Error: {0}")]
    JsonParseError(#[from] serde_json::Error),
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Error parsing stdout to str: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
}

pub trait YtDlp {
    async fn search_for_video(&self, query: VideoQuery) -> Result<VideoInfo, YtDlpError>;
    async fn search_for_playlist(&self, url: &str) -> Result<Vec<VideoInfo>, YtDlpError>;
    async fn get_audio_streams(&self, info: &VideoInfo) -> Result<VideoStreamInfo, YtDlpError>;
}
