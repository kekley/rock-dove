use std::{collections::HashMap, fmt};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Deserializer, Serialize};

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
#[derive(Deserialize, Debug, Serialize, Clone, PartialEq)]
pub struct Thumbnail {}

#[derive(Deserialize, Debug, Serialize, Clone, PartialEq)]
pub struct AutomaticCaption {}

///The output of 'yt-dlp'.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub thumbnail: String,
    pub description: String,
    pub availability: String,
    #[serde(rename = "timestamp")]
    pub upload_date: i64,
    pub view_count: i64,
    pub like_count: Option<i64>,
    pub comment_count: Option<i64>,
    pub channel: String,
    pub channel_id: String,
    pub channel_url: String,
    pub channel_follower_count: Option<i64>,
    pub formats: Vec<Format>,
    pub thumbnails: Vec<Thumbnail>,
    pub automatic_captions: HashMap<String, Vec<AutomaticCaption>>,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub age_limit: i64,
    #[serde(rename = "_has_drm")]
    pub has_drm: Option<bool>,
    pub live_status: String,
    pub playable_in_embed: bool,
    #[serde(flatten)]
    pub extractor_info: ExtractorInfo,
    #[serde(rename = "_version")]
    pub version: Version,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractorInfo {
    pub extractor: String,
    pub extractor_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Version {
    pub version: String,
    pub current_git_head: Option<String>,
    pub release_git_head: Option<String>,
    pub repository: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Format {
    pub format: String,
    pub format_id: String,
    pub format_note: Option<String>,
    #[serde(default)]
    pub protocol: Protocol,
    pub language: Option<String>,
    pub has_drm: Option<bool>,
    #[serde(default)]
    pub container: Option<Container>,
    #[serde(flatten)]
    pub codec_info: CodecInfo,
    #[serde(flatten)]
    pub video_resolution: VideoResolution,
    #[serde(flatten)]
    pub download_info: DownloadInfo,
    #[serde(flatten)]
    pub quality_info: QualityInfo,
    #[serde(flatten)]
    pub file_info: FileInfo,
    #[serde(flatten)]
    pub storyboard_info: StoryboardInfo,
    #[serde(flatten)]
    pub rates_info: RatesInfo,
    #[serde(skip)]
    pub video_id: Option<String>,
}

impl Format {
    pub fn is_video(&self) -> bool {
        let format_type = self.format_type();

        format_type.is_video()
    }

    pub fn is_audio(&self) -> bool {
        let format_type = self.format_type();

        format_type.is_audio()
    }

    pub fn format_type(&self) -> FormatType {
        if self.download_info.manifest_url.is_some() {
            return FormatType::Manifest;
        }

        if self.storyboard_info.fragments.is_some() {
            return FormatType::Storyboard;
        }

        let audio = self.codec_info.audio_codec.is_some();
        let video = self.codec_info.video_codec.is_some();

        match (audio, video) {
            (true, true) => FormatType::AudioVideo,
            (true, false) => FormatType::Audio,
            (false, true) => FormatType::Video,
            _ => FormatType::Manifest,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Format(id = {}, format = {})",
            self.format_id, self.format
        )
    }
}

/// Represents the codec information of a format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CodecInfo {
    #[serde(default)]
    #[serde(rename = "acodec")]
    #[serde(deserialize_with = "json_none")]
    pub audio_codec: Option<String>,
    #[serde(default)]
    #[serde(rename = "vcodec")]
    #[serde(deserialize_with = "json_none")]
    pub video_codec: Option<String>,
    #[serde(default)]
    pub audio_ext: Extension,
    #[serde(default)]
    pub video_ext: Extension,
    pub audio_channels: Option<i64>,
    pub asr: Option<i64>,
}

impl fmt::Display for CodecInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CodecInfo(audio = {}, video = {})",
            self.audio_codec.as_deref().unwrap_or("none"),
            self.video_codec.as_deref().unwrap_or("none")
        )
    }
}

/// Represents the video resolution information of a format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VideoResolution {
    /// The width of the video.
    pub width: Option<u32>,
    /// The height of the video.
    pub height: Option<u32>,
    /// The combined resolution of the video, e.g. '1920x1080' or 'audio only'.
    pub resolution: Option<String>,
    /// The frames per second of the video, e.g. '24' or '25'.
    pub fps: Option<OrderedFloat<f64>>,
    /// The aspect ratio of the video, e.g. '1.77' or '1.78' (corresponding to 16:9).
    pub aspect_ratio: Option<OrderedFloat<f64>>,
}

impl fmt::Display for VideoResolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.width, self.height) {
            (Some(w), Some(h)) => write!(f, "VideoResolution(width = {}, height = {})", w, h),
            _ => write!(f, "VideoResolution(unknown)"),
        }
    }
}

/// Represents the download information of a format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DownloadInfo {
    /// The decrypted URL of the format.
    pub url: Option<String>,
    /// The extension of the format.
    #[serde(default)]
    pub ext: Extension,
    /// The HTTP headers used by the downloader.
    pub http_headers: HttpHeaders,
    /// The manifest URL, if the format is a manifest.
    pub manifest_url: Option<String>,
    /// The options used by the downloader.
    pub downloader_options: Option<DownloaderOptions>,
}

impl fmt::Display for DownloadInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(url) = &self.url {
            write!(f, "DownloadInfo(url = {})", url)
        } else {
            write!(f, "DownloadInfo(no_url)")
        }
    }
}

/// Represents the quality information of a format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QualityInfo {
    /// A relative quality score, e.g. '-1' (for example, if the format is a manifest) or '9.5'.
    pub quality: Option<OrderedFloat<f64>>,
    /// If the format is using a large dynamic range.
    #[serde(default)]
    pub dynamic_range: Option<DynamicRange>,
}

impl fmt::Display for QualityInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "QualityInfo(quality = {})",
            self.quality
                .map(|q| q.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )
    }
}

/// Represents the file information of a format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileInfo {
    /// The approximate file size of the format.
    pub filesize_approx: Option<i64>,
    /// The exact file size of the format.
    pub filesize: Option<i64>,
}

impl fmt::Display for FileInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(size) = self.filesize {
            write!(f, "FileInfo(size = {})", size)
        } else if let Some(approx) = self.filesize_approx {
            write!(f, "FileInfo(approx_size = {})", approx)
        } else {
            write!(f, "FileInfo(size = unknown)")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RatesInfo {
    #[serde(rename = "vbr")]
    pub video_rate: Option<OrderedFloat<f64>>,
    #[serde(rename = "abr")]
    pub audio_rate: Option<OrderedFloat<f64>>,
    #[serde(rename = "tbr")]
    pub total_rate: Option<OrderedFloat<f64>>,
}

impl fmt::Display for RatesInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RatesInfo(video = {}, audio = {}, total = {})",
            self.video_rate
                .map(|r| r.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.audio_rate
                .map(|r| r.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.total_rate
                .map(|r| r.to_string())
                .unwrap_or_else(|| "none".to_string())
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoryboardInfo {
    pub rows: Option<i64>,
    pub columns: Option<i64>,
    pub fragments: Option<Vec<Fragment>>,
}

impl fmt::Display for StoryboardInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.rows, self.columns) {
            (Some(r), Some(c)) => write!(f, "StoryboardInfo(rows = {}, columns = {})", r, c),
            _ => write!(f, "StoryboardInfo(unknown)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fragment {
    pub url: String,
    pub duration: OrderedFloat<f64>,
}

impl fmt::Display for Fragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Fragment(url = {}, duration = {})",
            self.url, self.duration
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DownloaderOptions {
    pub http_chunk_size: i64,
}

impl fmt::Display for DownloaderOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DownloaderOptions(chunk_size = {})",
            self.http_chunk_size
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HttpHeaders {
    #[serde(rename = "User-Agent")]
    pub user_agent: String,
    pub accept: String,
    #[serde(rename = "Accept-Language")]
    pub accept_language: String,
    #[serde(rename = "Sec-Fetch-Mode")]
    pub sec_fetch_mode: String,
}

impl fmt::Display for HttpHeaders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HttpHeaders(user_agent = {})", self.user_agent)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extension {
    M4A,
    Mp4,
    Webm,
    Mhtml,
    None,
    #[default]
    #[serde(other)]
    Unknown,
}

impl fmt::Display for Extension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Extension({})",
            match self {
                Extension::M4A => "m4a",
                Extension::Mp4 => "mp4",
                Extension::Webm => "webm",
                Extension::Mhtml => "mhtml",
                Extension::None => "none",
                Extension::Unknown => "unknown",
            }
        )
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    #[serde(rename = "webm_dash")]
    Webm,
    #[serde(rename = "m4a_dash")]
    M4A,
    #[serde(rename = "mp4_dash")]
    Mp4,
    #[default]
    #[serde(other)]
    Unknown,
}

impl fmt::Display for Container {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Container({})",
            match self {
                Container::Mp4 => "mp4",
                Container::Webm => "webm",
                Container::M4A => "m4a",
                Container::Unknown => "unknown",
            }
        )
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Https,
    #[serde(rename = "m3u8_native")]
    M3U8Native,
    Mhtml,
    #[default]
    #[serde(other)]
    Unknown,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Protocol({})",
            match self {
                Protocol::Https => "https",
                Protocol::M3U8Native => "hls",
                Protocol::Mhtml => "mhtml",
                Protocol::Unknown => "unknown",
            }
        )
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DynamicRange {
    SDR,
    HDR,
    #[default]
    #[serde(other)]
    Unknown,
}

impl fmt::Display for DynamicRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DynamicRange({})",
            match self {
                DynamicRange::SDR => "SDR",
                DynamicRange::HDR => "HDR",
                DynamicRange::Unknown => "Unknown",
            }
        )
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormatType {
    Audio,
    Video,
    AudioVideo,
    Manifest,
    Storyboard,

    #[default]
    #[serde(other)]
    Unknown,
}

impl fmt::Display for FormatType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FormatType({})",
            match self {
                FormatType::Audio => "Audio",
                FormatType::Video => "Video",
                FormatType::AudioVideo => "AudioVideo",
                FormatType::Manifest => "Manifest",
                FormatType::Storyboard => "Storyboard",
                FormatType::Unknown => "Unknown",
            }
        )
    }
}

impl FormatType {
    pub fn is_audio_and_video(&self) -> bool {
        matches!(self, FormatType::AudioVideo)
    }

    pub fn is_video(&self) -> bool {
        matches!(self, FormatType::Video)
    }

    pub fn is_audio(&self) -> bool {
        matches!(self, FormatType::Audio)
    }

    pub fn is_storyboard(&self) -> bool {
        matches!(self, FormatType::Storyboard)
    }

    pub fn is_manifest(&self) -> bool {
        matches!(self, FormatType::Manifest)
    }
}
