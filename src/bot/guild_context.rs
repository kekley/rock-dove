use std::{collections::VecDeque, error::Error, fmt::Debug, ops::RangeBounds, sync::Arc};

use rand::{rng, seq::SliceRandom};
use reqwest::{Client, header::HeaderMap};
use serenity::{
    all::{ChannelId, Context, GuildId, UserId},
    async_trait,
};
use songbird::{
    TrackEvent,
    error::ControlError,
    input::{
        AudioStream, AudioStreamError, AuxMetadata, Compose, HlsRequest, HttpRequest, Input,
        core::io::MediaSource,
    },
};
use tokio::sync::RwLock;
use tracing::{Level, event};

use crate::{
    bot::{
        TrackEndNotifier, TrackErrorNotifier, send_message,
        tracks::{PlayingTrack, QueuedTrack},
        undo_stack::{UndoData, UndoStack},
    },
    yt_dlp::format::Protocol,
};

#[derive(Default)]
pub struct GuildContext {
    pub start_pattern: String,
    pub playback_queue: PlaybackQueue,
    current_track: Option<PlayingTrack>,
    pub undo_stack: UndoStack,
    loop_mode: LoopMode,
}

pub enum TrackControlResult {
    Success,
    NoTrack,
    Error(ControlError),
}

impl GuildContext {
    pub fn new() -> Self {
        Self {
            start_pattern: "*".to_string(),
            ..Default::default()
        }
    }
    pub fn queue_len(&self) -> usize {
        self.playback_queue.data.len()
    }
    pub fn current_queue_pos(&self) -> usize {
        self.playback_queue.queue_position + 1
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
    ///this should only be called from a handler that ensures the current track has ended or
    ///errored
    pub async fn handle_next_track(&mut self, ctx: &Context, guild_id: GuildId) {
        let manager = songbird::get(ctx)
            .await
            .expect("songbird should have been inserted at startup");

        if let Some(call) = manager.get(guild_id) {
            println!("got call");
            let mut call_lock = call.lock().await;
            if let Some(channel_id) = call_lock.current_channel() {
                println!("got channel");
                let voice_states = guild_id
                    .to_guild_cached(ctx)
                    .map(|g| g.voice_states.clone());
                if voice_states.is_none() {
                    println!("no voice_states");
                    //Empty the current call slot and return, let
                    let _ = self.current_track.take();
                    let _ = call_lock.leave().await;
                    return;
                }

                //See if anyone is in the call with us
                if voice_states
                    .expect("This should be some")
                    .iter()
                    .any(|(_, state)| {
                        state
                            .channel_id
                            .is_some_and(|id| songbird::id::ChannelId::from(id) == channel_id)
                    })
                {
                    if let Some(track) = self.playback_queue.next_track() {
                        //Play the next track
                        let handle = call_lock.play_input(track.audio.as_ref().clone().into());
                        self.current_track = Some(PlayingTrack {
                            handle,
                            stream: track.audio.clone(),
                        });
                    } else {
                        println!("queue ended");
                        //Queue ended. Clear it
                        self.playback_queue.clear();
                        self.undo_stack.clear();
                        let _ = self.current_track.take();
                    }
                } else {
                    println!("no one in call");
                    //No one is in the call with us. clear the slot and leave
                    let _ = self.current_track.take();
                    let _ = call_lock.leave().await;
                }
            }
        } else {
            println!("not in call");
            //We're not in a call. don't play the next track
            let _ = self.current_track.take();
        }
    }
    pub async fn pause(&mut self) -> TrackControlResult {
        if let Some(current_track) = &self.current_track {
            match current_track.handle.pause() {
                Ok(()) => TrackControlResult::Success,
                Err(err) => TrackControlResult::Error(err),
            }
        } else {
            TrackControlResult::NoTrack
        }
    }
    pub async fn resume(&mut self) -> TrackControlResult {
        if let Some(current_track) = &self.current_track {
            match current_track.handle.play() {
                Ok(()) => TrackControlResult::Success,
                Err(err) => TrackControlResult::Error(err),
            }
        } else {
            TrackControlResult::NoTrack
        }
    }
    pub async fn mute(&mut self, ctx: &Context, guild_id: GuildId) {
        let songbird_manager = songbird::get(ctx)
            .await
            .expect("Songbird manager should have been inserted at startup");
        if let Some(call) = songbird_manager.get(guild_id) {
            let mut lock = call.lock().await;
            let _ = lock.mute(true).await;
        }
    }

    pub async fn unmute(&mut self, ctx: &Context, guild_id: GuildId) {
        let songbird_manager = songbird::get(ctx)
            .await
            .expect("Songbird manager should have been inserted at startup");
        if let Some(call) = songbird_manager.get(guild_id) {
            let mut lock = call.lock().await;
            let _ = lock.mute(false).await;
        }
    }

    pub async fn set_loop_mode(&mut self, loop_mode: LoopMode) {
        self.loop_mode = loop_mode;
    }

    pub async fn play_now(
        &mut self,
        ctx: &Context,
        guild_id: GuildId,
        request_voice_channel: ChannelId,
        track: Arc<StreamData>,
    ) {
        if let Some(current_track) = self.current_track.take() {
            let _ = current_track.handle.stop();
        }

        let manager = songbird::get(ctx)
            .await
            .expect("Songbird manager should have been registered");

        if let Some(call_manager_mutex) = manager.get(guild_id) {
            let mut call_manager = call_manager_mutex.lock().await;
            if call_manager
                .current_channel()
                .is_some_and(|id| id == request_voice_channel.into())
            {
                let track_handle = call_manager.play_input(track.as_ref().clone().into());
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
                self.current_track = Some(PlayingTrack::new(track_handle, track));
            }
        }
    }

    pub async fn redo(&mut self) -> bool {
        if let Some(state) = self.undo_stack.pop_redo() {
            self.playback_queue = state.queue;
            true
        } else {
            false
        }
    }

    pub async fn shuffle_queue(&mut self) {
        self.playback_queue.shuffle();
        self.push_current_state_to_undo_stack().await;
    }

    //Ends the current track
    pub async fn next_track(
        &mut self,
        request_text_channel: ChannelId,
        ctx: &Context,
        guild_id: GuildId,
    ) {
        if let Some(current_track) = self.current_track.take() {
            match current_track.handle.stop() {
                Ok(_) => {
                    send_message(request_text_channel, &ctx.http, "Skipped track").await;
                }
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    event!(Level::ERROR, "Could not stop current track. Error:{err}");
                }
            }
            self.handle_next_track(ctx, guild_id).await;
        } else {
            send_message(
                request_text_channel,
                &ctx.http,
                "No track currently playing",
            )
            .await;
        }
    }

    pub async fn add_to_queue(
        &mut self,
        user: UserId,
        audio: Arc<StreamData>,
        ctx: &Context,
        guild_id: GuildId,
    ) {
        self.playback_queue.add_to_back(user, audio);
        if self.get_current_track_info().is_none() {
            self.handle_next_track(ctx, guild_id).await;
        }
        self.push_current_state_to_undo_stack().await;
    }

    async fn push_current_state_to_undo_stack(&mut self) {
        let state = UndoData {
            queue: self.playback_queue.clone(),
        };
        self.undo_stack.push_undo(state);
    }
    ///Returns true if there was a state to restore
    async fn restore_state_from_undo_stack(&mut self) -> bool {
        let Some(new_state) = self.undo_stack.pop_undo() else {
            return false;
        };
        let UndoData { queue } = new_state;

        self.playback_queue = queue;
        true
    }
    ///Clears the queue
    pub async fn clear_queue(&mut self) {
        self.playback_queue.clear();
        self.push_current_state_to_undo_stack().await;
    }

    ///Public undo function
    pub async fn undo(&mut self) -> bool {
        self.restore_state_from_undo_stack().await
    }

    ///Joins the voice channel a command came from.
    pub async fn handle_voice_channel_joining(
        request_guild: GuildId,
        request_text_channel: ChannelId,
        request_voice_channel: ChannelId,
        guild_context: Arc<RwLock<GuildContext>>,
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
                    TrackErrorNotifier {
                        guild: guild_context.clone(),
                        context: ctx.clone(),
                        guild_id: request_guild,
                    },
                );
                call_lock.add_global_event(
                    TrackEvent::End.into(),
                    TrackEndNotifier {
                        guild_context: guild_context.clone(),
                        context: ctx.clone(),
                        guild_id: request_guild,
                    },
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
                unreachable!()
            }
        }
    }
}

impl GuildContext {
    pub fn queue_is_empty(&self) -> bool {
        self.playback_queue.is_empty()
    }
    pub fn queue_position(&self) -> usize {
        self.playback_queue.queue_position
    }
    pub fn iter_queue(&self) -> impl Iterator<Item = &QueuedTrack> {
        self.playback_queue.data.iter()
    }
    pub fn get_current_track_info(&self) -> Option<&PlayingTrack> {
        self.current_track.as_ref()
    }
    pub fn remove_tracks_in_range<R>(&mut self, range: R) -> usize
    where
        R: RangeBounds<usize> + Debug,
    {
        let drain = self.playback_queue.data.drain(range);
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
        let starting_len = self.playback_queue.data.len();

        //A vector of bools indicating whether a track is to be removed. We do it this way because
        // the closure passed to .retain can't be async
        let mut removal_vec = Vec::with_capacity(starting_len);
        for queued_track in &self.playback_queue.data {
            let user_id = queued_track.user;
            match user_id.to_user(&ctx.http).await {
                Ok(user) => {
                    let username = user.name.as_str();
                    let nick = user.nick_in(&ctx.http, guild_id).await;
                    let username_similarity = strsim::jaro_winkler(username, user_arg);
                    let nickname_similarity = nick
                        .map(|string| strsim::jaro_winkler(&string, user_arg))
                        .unwrap_or(0.0);
                    let removal = username_similarity >= 0.9 || nickname_similarity >= 0.9;
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
        self.playback_queue.data.retain(|_| {
            //Negate the result because retain removes items that return false
            let result = !removal_vec
                .get(i)
                .expect("removal_vec should be the same size as our queue");
            i += 1;
            result
        });

        let ending_len = self.playback_queue.data.len();

        self.push_current_state_to_undo_stack().await;
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
