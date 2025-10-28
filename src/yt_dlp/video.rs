use crate::bot::guild_context::StreamData;
use crate::yt_dlp::extractor_info::ExtractorInfo;
use crate::yt_dlp::thumbnail::Thumbnail;
use crate::yt_dlp::{automatic_caption::AutomaticCaption, format::Format};
use std::collections::HashMap;

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

///The output json of yt-dlp
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Video<'a> {
    #[serde(borrow)]
    pub id: &'a str,
    #[serde(borrow)]
    pub title: &'a str,
    #[serde(borrow)]
    pub thumbnail: &'a str,
    #[serde(borrow)]
    pub description: &'a str,
    pub availability: &'a str,
    #[serde(rename = "timestamp")]
    pub upload_date: i64,
    pub view_count: i64,
    pub like_count: Option<i64>,
    pub comment_count: Option<i64>,
    #[serde(borrow)]
    pub channel: &'a str,
    #[serde(borrow)]
    pub channel_id: &'a str,
    #[serde(borrow)]
    pub channel_url: &'a str,
    pub channel_follower_count: Option<i64>,
    pub formats: Vec<Format>,
    pub thumbnails: Vec<Thumbnail>,
    pub automatic_captions: HashMap<String, Vec<AutomaticCaption>>,
    #[serde(borrow)]
    pub tags: Vec<&'a str>,
    #[serde(borrow)]
    pub categories: Vec<&'a str>,
    pub age_limit: i64,
    #[serde(rename = "_has_drm")]
    pub has_drm: Option<bool>,
    pub live_status: &'a str,
    pub playable_in_embed: bool,
    #[serde(flatten)]
    pub extractor_info: ExtractorInfo,
    #[serde(rename = "_version")]
    pub version: Version<'a>,
}

impl<'a> Video<'a> {
    pub fn to_audio_stream(&self, client: Client) -> Option<StreamData> {
        let metadata = self.metadata();
        self.formats
            .iter()
            .filter(|format| format.is_audio() && format.download_info.url.is_some())
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
                    name: self.title.to_string(),
                    url: f.download_info.url.as_ref()?.to_string(),
                    headers,
                    protocol: f.protocol.clone(),
                    client,
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
pub struct Version<'a> {
    #[serde(borrow)]
    version: &'a str,
    #[serde(borrow)]
    current_git_head: Option<&'a str>,
    #[serde(borrow)]
    release_git_head: Option<&'a str>,
    #[serde(borrow)]
    repository: &'a str,
}
