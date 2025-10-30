use std::{
    collections::VecDeque, error::Error, ops::RangeBounds, slice::SliceIndex, sync::Arc, vec,
};

use reqwest::{Client, header::HeaderMap};
use serenity::{
    all::{ChannelId, Context, GuildId, UserId},
    async_trait,
    futures::future::join_all,
};
use songbird::{
    TrackEvent,
    input::{
        AudioStream, AudioStreamError, AuxMetadata, Compose, HlsRequest, HttpRequest, Input,
        core::io::MediaSource,
    },
    tracks::TrackHandle,
};
use symphonia::core::units::Duration;
use tracing::{Level, event};

use crate::{
    bot::{
        TrackErrorNotifier, send_message,
        tracks::{PlayingTrack, QueuedTrack},
        undo_stack::{UndoData, UndoStack},
    },
    yt_dlp::format::Protocol,
};

#[derive(Debug, Clone, Copy, Default)]
pub enum LoopMode {
    Track,
    Queue,
    #[default]
    Off,
}

pub enum RemoveMode {
    From,
    At,
    Until,
    Past,
}

#[derive(Debug, Clone)]
pub struct PlaybackQueue {
    data: VecDeque<QueuedTrack>,
    queue_position: usize,
}
#[derive(Default)]
pub struct GuildContext {
    pub start_pattern: String,
    playback_queue: VecDeque<QueuedTrack>,
    current_track: Option<PlayingTrack>,
    undo_stack: UndoStack,
    loop_mode: LoopMode,
}

impl GuildContext {
    pub fn new() -> Self {
        Self {
            start_pattern: "*".to_string(),
            ..Default::default()
        }
    }
    pub async fn has_track_playing(&self) -> bool {
        if let Some(track) = &self.current_track {
            match track.handle.get_info().await {
                Ok(info) => !info.playing.is_done(),
                Err(err) => {
                    eprint!("Error getting track state: {err}");
                    false
                }
            }
        } else {
            false
        }
    }
    pub async fn pause(&mut self) {
        if let Some(current_track) = self.current_track {}
    }

    pub async fn set_loop_mode(&mut self, loop_mode: LoopMode) {
        if let Some(track) = &self.current_track {}
        self.loop_mode = loop_mode;
    }

    pub async fn resume(&mut self) {
        if let Some(track) = &self.current_track {}
    }
    pub async fn play_now(
        &mut self,
        ctx: &Context,
        guild_id: GuildId,
        request_voice_channel: ChannelId,
        request_text_channel: ChannelId,
        track: StreamData,
    ) {
        if self.has_track_playing().await {
            self.current_track
                .as_ref()
                .expect("track handle must be Some by this point")
                .0
                .pause()
                .unwrap();
        }

        //backup current state
        self.push_current_state_to_undo_stack();

        let manager = songbird::get(&ctx)
            .await
            .expect("Songbird manager should have been registered");

        if let Some(call_manager_mutex) = manager.get(guild_id) {
            let mut call_manager = call_manager_mutex.lock().await;
            if call_manager
                .current_channel()
                .is_some_and(|id| id == request_voice_channel.into())
            {
                let track_handle = call_manager.play_input(track.clone().into());
                let result = track_handle.make_playable();
                match result.result_async().await {
                    Ok(_) => {}
                    Err(err) => {
                        #[cfg(feature = "tracing")]
                        event!(Level::ERROR, "Could not play track: {err}");
                        send_message(
                            request_text_channel,
                            &ctx.http,
                            "COUGH WHEEEZE I'M FUCKING DEAD (that didn't work for some reason, sorry)",
                        )
                        .await;
                        return;
                    }
                }
                match track_handle.play() {
                    Ok(_) => {
                        #[cfg(feature = "tracing")]
                        event!(Level::INFO, "Started track playback successfully");

                        send_message(
                            request_text_channel,
                            &ctx.http,
                            format!("Started playing: {track_name}", track_name = track.name)
                                .as_str(),
                        )
                        .await;
                    }
                    Err(err) => {
                        #[cfg(feature = "tracing")]
                        event!(Level::ERROR, "Could not play track: {err}");
                        send_message(
                            request_text_channel,
                            &ctx.http,
                            "COUGH WHEEEZE I'M FUCKING DEAD (that didn't work for some reason, sorry)",
                        )
                        .await;
                        return;
                    }
                }
                if let LoopMode::Track = self.loop_mode {
                    match track_handle.enable_loop() {
                        Ok(()) => {
                            #[cfg(feature = "tracing")]
                            event!(Level::INFO, "Enabled track looping");
                        }
                        Err(err) => {
                            #[cfg(feature = "tracing")]
                            event!(Level::ERROR, "Could not enable track looping: {err}");
                        }
                    }
                };
                self.current_track = Some((track_handle, track.clone()));
            }
        }
    }

    pub fn shuffle_queue(&mut self) {
        todo!()
    }
    pub async fn skip_track(&mut self) {}

    pub fn add_to_queue(&mut self, user: UserId, track: StreamData) {
        self.push_current_state_to_undo_stack();
        self.playback_queue.push_back(QueuedTrack {
            user,
            stream: track.into(),
        });
    }

    fn push_current_state_to_undo_stack(&mut self) {
        let state = UndoData {
            current_track: todo!(),
            queue: todo!(),
        };
    }
    fn restore_state_from_undo_stack(&mut self) {}
    fn cancel_undo_push(&mut self) {}
    pub fn clear_queue(&mut self) {
        self.playback_queue.clear();
    }

    pub async fn handle_voice_channel_joining(
        request_guild: GuildId,
        request_text_channel: ChannelId,
        request_voice_channel: ChannelId,
        ctx: &Context,
    ) {
        let manager = songbird::get(ctx)
            .await
            .expect("Songbird Voice client placed in at initialisation.")
            .clone();

        match manager.get(request_guild) {
            Some(call) => {
                let call_lock = call.lock().await;
                if let Some(current_channel) = call_lock.current_channel()
                    && current_channel == request_voice_channel.into()
                {
                    //Already in the correct call wheeeeeeee
                    return;
                }
            }
            _ => {
                //fall through to call joining code
            }
        };

        match manager.join(request_guild, request_voice_channel).await {
            Ok(call) => {
                let mut call_lock = call.lock().await;
                call_lock.add_global_event(
                    TrackEvent::Error.into(),
                    TrackErrorNotifier { guild: todo!() },
                );
            }
            Err(err) => {
                #[cfg(feature = "tracing")]
                event!(Level::ERROR, "Failed to join voice call. Error: {err}");
                send_message(
                    request_text_channel,
                    &ctx.http,
                    "Couldn't join the voice call you're in :(",
                )
                .await;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct StreamData {
    pub name: Box<str>,
    pub url: Box<str>,
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
                #[cfg(feature = "tracing")]
                event!(Level::INFO, "Playing m3u8 stream");

                let mut request = HlsRequest::new_with_headers(
                    self.client.clone(),
                    self.url.to_string(),
                    self.headers.clone(),
                );
                request.create()
            }
            Protocol::Https => {
                #[cfg(feature = "tracing")]
                event!(Level::INFO, "Playing https stream");
                let mut req = HttpRequest {
                    client: self.client.clone(),
                    request: self.url.to_string(),
                    headers: self.headers.clone(),
                    content_length: self.file_size,
                };
                req.create_async().await
            }
            _ => {
                panic!()
            }
        }
    }
}

impl GuildContext {
    pub fn queue_is_empty(&self) -> bool {
        self.playback_queue.is_empty()
    }
    pub fn iter_queue(&self) -> impl Iterator<Item = &QueuedTrack> {
        self.playback_queue.iter()
    }
    pub fn get_current_track_info(&self) -> Option<&PlayingTrack> {
        self.current_track
    }
    pub fn remove_tracks_in_range<R>(&mut self, range: R) -> usize
    where
        R: RangeBounds<usize>,
    {
        let drain = self.playback_queue.drain(range);
        drain.len()
    }
    ///Remove tracks added by a certain user. Uses a string similarity test to account for typos or
    ///usernames that might contain hard to type symbols
    pub async fn remove_tracks_from(
        &mut self,
        guild_id: GuildId,
        user_arg: &str,
        ctx: &Context,
    ) -> usize {
        self.push_current_state_to_undo_stack();
        let starting_len = self.playback_queue.len();

        //A vector of bools indicating whether a track is to be removed. We do it this way because
        // the closure passed to .retain can't be async
        let mut removal_vec = Vec::with_capacity(starting_len);
        for queued_track in &self.playback_queue {
            let user_id = queued_track.user;
            match user_id.to_user(&ctx.http).await {
                Ok(user) => {
                    let username = user.name.as_str();
                    let nick = user.nick_in(&ctx.http, guild_id).await;
                    let username_similarity = strsim::jaro_winkler(username, user_arg);
                    let nickname_similarity = nick
                        .map(|string| strsim::jaro_winkler(&string, user_arg))
                        .unwrap_or(0.0);
                    let removal = if username_similarity >= 0.9 || nickname_similarity >= 0.9 {
                        true
                    } else {
                        false
                    };
                    removal_vec.push(removal);
                }
                Err(err) => {
                    //If the http request fails we probably want to aid on the side of keeping the
                    //track
                    #[cfg(feature = "tracing")]
                    event!(
                        Level::WARN,
                        "HTTP request failed while removing track for user. Error: {err}"
                    );

                    removal_vec.push(true);
                }
            }
        }
        let mut i = 0;
        self.playback_queue.retain(|_| {
            //Negate the result because retain removes items that return false
            let result = !removal_vec
                .get(i)
                .expect("removal_vec should be the same size as our queue");
            i += 1;
            result
        });

        let ending_len = self.playback_queue.len();
        starting_len - ending_len
    }
}

impl From<StreamData> for Input {
    fn from(val: StreamData) -> Self {
        Input::Lazy(Box::new(val))
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
