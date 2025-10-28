use std::error::Error;

use reqwest::{Client, header::HeaderMap};
use serenity::async_trait;
use songbird::{
    id::UserId,
    input::{
        AudioStream, AudioStreamError, AuxMetadata, Compose, HlsRequest, HttpRequest, Input,
        core::io::MediaSource,
    },
    tracks::TrackHandle,
};

use crate::yt_dlp::format::Protocol;

#[derive(Default)]
pub struct GuildContext {
    pub start_pattern: String,
    playback_queue: Vec<QueueEntry>,
    current_voice_channel: Option<()>,
    current_playback: Option<PlaybackTask>,
    track_handle: Option<TrackHandle>,
}

impl GuildContext {
    pub fn new() -> Self {
        Self {
            start_pattern: "*".to_string(),
            ..Default::default()
        }
    }
}

pub struct QueueEntry {
    pub user: UserId,
    pub stream: StreamData,
}
pub struct PlaybackTask {}

pub struct QueuePosition(usize);
pub struct StreamData {
    pub name: String,
    pub url: String,
    pub client: Client,
    pub headers: HeaderMap,
    pub file_size: Option<u64>,
    pub protocol: Protocol,
    pub metadata: Option<AuxMetadata>,
}

impl StreamData {
    pub async fn get_stream(&self) -> Result<AudioStream<Box<dyn MediaSource>>, AudioStreamError> {
        match self.protocol {
            Protocol::M3U8Native => {
                let mut request = HlsRequest::new_with_headers(
                    self.client.clone(),
                    self.url.clone(),
                    self.headers.clone(),
                );
                request.create()
            }
            _ => {
                let mut req = HttpRequest {
                    client: self.client.clone(),
                    request: self.url.clone(),
                    headers: self.headers.clone(),
                    content_length: self.file_size,
                };
                req.create_async().await
            }
        }
    }
}

impl GuildContext {
    pub fn queue_is_empty(&self) -> bool {
        self.playback_queue.is_empty()
    }
    pub fn iter_queue(&self) -> impl Iterator<Item = &QueueEntry> {
        self.playback_queue.iter()
    }
}

impl Into<Input> for StreamData {
    fn into(self) -> Input {
        Input::Lazy(Box::new(self))
    }
}

#[async_trait]
impl Compose for StreamData {
    fn create(&mut self) -> Result<AudioStream<Box<dyn MediaSource>>, AudioStreamError> {
        Err(AudioStreamError::Unsupported)
    }

    async fn create_async(
        &mut self,
    ) -> Result<AudioStream<Box<dyn MediaSource>>, AudioStreamError> {
        self.get_stream().await
    }

    fn should_create_async(&self) -> bool {
        true
    }

    async fn aux_metadata(&mut self) -> Result<AuxMetadata, AudioStreamError> {
        if let Some(meta) = self.metadata.as_ref() {
            return Ok(meta.clone());
        }

        self.metadata.clone().ok_or_else(|| {
            let msg: Box<dyn Error + Send + Sync + 'static> =
                "Failed to instansiate any metadata... Should be unreachable.".into();
            AudioStreamError::Fail(msg)
        })
    }
}
