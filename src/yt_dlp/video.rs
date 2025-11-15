use crate::bot::guild_context::StreamData;
use crate::yt_dlp::format::{Format, Protocol};

use ordered_float::OrderedFloat;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Deserializer, Serialize};
use songbird::input::AuxMetadata;

pub fn json_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let string: Option<String> = Option::deserialize(deserializer)?;

    match string.as_deref() {
        Some("none") => Ok(None),
        _ => Ok(string),
    }
}

///The output json of yt-dlp that we can get an audio stream from
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoStreamInfo {
    pub title: String,
    pub thumbnail: String,
    pub channel: String,
    pub duration_string: String,
    pub formats: Vec<Format>,
}

impl VideoStreamInfo {
    pub fn to_audio_stream(self, client: Client) -> Option<StreamData> {
        let metadata = self.metadata();
        self.formats
            .into_iter()
            .filter(|format| {
                format.is_audio()
                    && format.download_info.url.is_some()
                    && matches!(format.protocol, Protocol::M3U8Native | Protocol::Https)
            })
            .max_by(|a, b| {
                a.quality_info
                    .quality
                    .unwrap_or(OrderedFloat(0.0))
                    .cmp(&b.quality_info.quality.unwrap_or(OrderedFloat(0.0)))
            })
            .and_then(|f| {
                let mut headers = HeaderMap::new();
                headers.insert(
                    "User-Agent",
                    HeaderValue::from_str(f.download_info.http_headers.user_agent.as_str()).ok()?,
                );
                headers.insert(
                    "Accept",
                    HeaderValue::from_str(f.download_info.http_headers.accept.as_str()).ok()?,
                );
                headers.insert(
                    "Accept-Language",
                    HeaderValue::from_str(f.download_info.http_headers.accept_language.as_str())
                        .ok()?,
                );
                headers.insert(
                    "Sec-Fetch-Mode",
                    HeaderValue::from_str(f.download_info.http_headers.accept_language.as_str())
                        .ok()?,
                );
                Some(StreamData {
                    name: self.title.into(),
                    url: f.download_info.url?.into(),
                    headers,
                    protocol: f.protocol.clone(),
                    client,
                    duration_string: self.duration_string.into_boxed_str(),
                    file_size: f.file_info.filesize.map(|i| i as u64),
                    metadata: Some(metadata),
                })
            })
    }

    fn metadata(&self) -> AuxMetadata {
        AuxMetadata {
            track: Some(self.title.to_string()),
            channels: Some(2),
            channel: Some(self.channel.to_string()),
            sample_rate: Some(48000),
            title: Some(self.title.to_string()),
            thumbnail: Some(self.thumbnail.to_string()),
            ..Default::default()
        }
    }
}

///yt-dlp version
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Version {
    version: String,
    current_git_head: Option<String>,
    release_git_head: Option<String>,
    repository: String,
}
