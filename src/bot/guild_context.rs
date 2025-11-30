use std::{error::Error, fmt::Debug, ops::RangeBounds, sync::Arc};

use reqwest::{Client, header::HeaderMap};
use serenity::{
    all::{ChannelId, Context, GuildId, UserId},
    async_trait,
};
use songbird::{
    TrackEvent,
    error::{ControlError, JoinError},
    input::{
        AudioStream, AudioStreamError, AuxMetadata, Compose, HlsRequest, HttpRequest, Input,
        core::io::MediaSource,
    },
};
use thiserror::Error;
use tracing::{Level, event};

use crate::{
    HTTPClientKey,
    bot::{
        command::get_songbird,
        queue::{LoopMode, PlaybackQueue},
        send_message,
        track_notifier::{TrackEndNotifier, TrackErrorNotifier},
        tracks::{PlayingTrack, QueuedTrack},
        undo_stack::{UndoData, UndoStack},
    },
    yt_dlp::{YtDlp, YtDlpKey, format::Protocol, playlist::VideoInfo},
};

pub struct GuildContext {
    pub start_pattern: String,
    pub playback_queue: PlaybackQueue,
    current_track: Option<PlayingTrack>,
    pub undo_stack: UndoStack,
    loop_mode: LoopMode,
}

#[derive(Debug, Error)]
pub enum TrackControlError {
    #[error("No track to control")]
    NoTrack,
    #[error("Control error: {0}")]
    Error(#[from] ControlError),
}

#[derive(Debug, Error)]
pub enum BotControlError {
    #[error("Bot was not in a call")]
    NotInCall,
    #[error("Songbird JoinError: {0}")]
    JoinError(#[from] JoinError),
}

impl Default for GuildContext {
    fn default() -> Self {
        Self {
            start_pattern: "*".to_string(),
            playback_queue: Default::default(),
            current_track: Default::default(),
            undo_stack: Default::default(),
            loop_mode: Default::default(),
        }
    }
}

impl GuildContext {
    pub fn queue_length(&self) -> usize {
        self.playback_queue.num_tracks()
    }
    pub fn queue_position(&self) -> usize {
        self.playback_queue.queue_position()
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

    pub async fn handle_next_track(&mut self, ctx: &Context, guild_id: GuildId) {
        let _ = self.end_current_track().await;
        let manager = songbird::get(ctx)
            .await
            .expect("songbird should have been inserted at startup");

        let read_lock = ctx.data.read().await;

        let yt_dlp = read_lock
            .get::<YtDlpKey>()
            .expect("YtDlp should have been set up at startup");
        let http_client = read_lock
            .get::<HTTPClientKey>()
            .expect("http client should have been set up at startup");

        if let Some(call) = manager.get(guild_id) {
            let mut call_lock = call.lock().await;
            if let Some(channel_id) = call_lock.current_channel() {
                let voice_states = guild_id
                    .to_guild_cached(ctx)
                    .map(|g| g.voice_states.clone());
                if voice_states.is_none() {
                    //Empty the current call slot and return
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
                            && !state
                                .user_id
                                .to_user_cached(&ctx.cache)
                                .is_some_and(|u| u.bot)
                    })
                {
                    if let Some(track) = self.playback_queue.next_track() {
                        //Play the next track
                        let stream_info = yt_dlp.get_audio_streams(&track.info).await.unwrap();
                        let stream = stream_info.to_audio_stream(http_client.clone()).unwrap();
                        let handle = call_lock.play_input(stream.clone().into());
                        self.current_track = Some(PlayingTrack {
                            handle,
                            stream: stream.into(),
                        });
                    } else {
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

    pub async fn pause_current_track(&mut self) -> Result<(), TrackControlError> {
        if let Some(current_track) = &self.current_track {
            match current_track.handle.pause() {
                Ok(_) => Ok(()),
                Err(err) => Err(TrackControlError::Error(err)),
            }
        } else {
            Err(TrackControlError::NoTrack)
        }
    }
    pub async fn resume_current_track(&mut self) -> Result<(), TrackControlError> {
        if let Some(current_track) = &self.current_track {
            match current_track.handle.play() {
                Ok(_) => Ok(()),
                Err(err) => Err(TrackControlError::Error(err)),
            }
        } else {
            Err(TrackControlError::NoTrack)
        }
    }
    pub async fn end_current_track(&mut self) -> Result<(), TrackControlError> {
        if let Some(current_track) = &self.current_track {
            match current_track.handle.stop() {
                Ok(_) => Ok(()),
                Err(err) => Err(TrackControlError::Error(err)),
            }
        } else {
            Err(TrackControlError::NoTrack)
        }
    }

    pub async fn mute(&mut self, ctx: &Context, guild_id: GuildId) -> Result<(), BotControlError> {
        let songbird_manager = songbird::get(ctx)
            .await
            .expect("Songbird manager should have been inserted at startup");
        if let Some(call) = songbird_manager.get(guild_id) {
            let mut lock = call.lock().await;
            Ok(lock.mute(true).await?)
        } else {
            Err(BotControlError::NotInCall)
        }
    }

    pub async fn unmute(
        &mut self,
        ctx: &Context,
        guild_id: GuildId,
    ) -> Result<(), BotControlError> {
        let songbird_manager = songbird::get(ctx)
            .await
            .expect("Songbird manager should have been inserted at startup");
        if let Some(call) = songbird_manager.get(guild_id) {
            let mut lock = call.lock().await;
            Ok(lock.mute(false).await?)
        } else {
            Err(BotControlError::NotInCall)
        }
    }

    pub async fn set_loop_mode(&mut self, loop_mode: LoopMode) {
        if let Some(current_track) = self.current_track.as_ref() {
            let _ = current_track.handle.enable_loop();
        }
        self.loop_mode = loop_mode;
    }

    pub async fn play_now(
        &mut self,
        ctx: Context,
        guild_id: GuildId,
        request_voice_channel: ChannelId,
        track: Arc<StreamData>,
    ) {
        if let Some(current_track) = self.current_track.take() {
            let _ = current_track.handle.stop();
        }

        let manager = songbird::get(&ctx)
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
        ctx: &Context,
        guild_id: GuildId,
    ) -> Result<(), TrackControlError> {
        if let Some(current_track) = self.current_track.take() {
            let _ = current_track.handle.stop();

            self.handle_next_track(ctx, guild_id).await;
            Ok(())
        } else {
            Err(TrackControlError::NoTrack)
        }
    }

    pub async fn add_to_queue(
        &mut self,
        user: UserId,
        video: Arc<VideoInfo>,
        ctx: &Context,
        guild_id: GuildId,
    ) {
        let track = QueuedTrack {
            added_by: user,
            info: video,
        };
        self.playback_queue.add_to_back(track);
        if self.get_current_track_info().is_none() {
            self.handle_next_track(ctx, guild_id).await;
        }
        self.push_current_state_to_undo_stack().await;
    }

    pub async fn add_many_to_queue(
        &mut self,
        user: UserId,
        streams: &[Arc<VideoInfo>],
        ctx: &Context,
        guild_id: GuildId,
    ) {
        for stream in streams {
            let track = QueuedTrack {
                added_by: user,
                info: stream.clone(),
            };
            self.playback_queue.add_to_back(track);
            if self.get_current_track_info().is_none() {
                self.handle_next_track(ctx, guild_id).await;
            }
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
        ctx: &Context,
    ) {
        let manager = get_songbird(ctx).await;

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
                        context: ctx.clone(),
                        guild_id: request_guild,
                    },
                );
                call_lock.add_global_event(
                    TrackEvent::End.into(),
                    TrackEndNotifier {
                        context: ctx.clone(),
                        guild_id: request_guild,
                    },
                );
            }
            Err(err) => {
                #[cfg(feature = "tracing")]
                event!(Level::ERROR, "Failed to join voice call. Error: {err}");
                send_message(
                    ctx,
                    request_text_channel,
                    "I Couldn't join the voice call you're in",
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
    pub duration_string: Box<str>,
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
    pub fn queue(&self) -> &PlaybackQueue {
        &self.playback_queue
    }
    pub fn queue_mut(&mut self) -> &mut PlaybackQueue {
        &mut self.playback_queue
    }
    pub fn get_current_track_info(&self) -> Option<&PlayingTrack> {
        self.current_track.as_ref()
    }

    ///Remove tracks added by a certain user. Uses a string similarity test to account for typos or
    ///usernames that might contain hard to type symbols
    pub async fn remove_tracks_from(
        &mut self,
        guild_id: GuildId,
        user_arg: &str,
        ctx: &Context,
    ) -> Result<usize, RemoveTracksFromError> {
        let Ok(members) = guild_id.members(&ctx.http, None, None).await else {
            #[cfg(feature = "tracing")]
            event!(Level::ERROR, "Error getting list of guild members");

            return Err(RemoveTracksFromError::ErrorFetchingMembers);
        };
        let name_matches = members
            .iter()
            .filter(|member| {
                let name = member.user.name.as_str();
                let name_similarity = strsim::jaro_winkler(name, user_arg);
                let nick = member.nick.as_ref();
                let nick_similarity = nick
                    .map(|str| strsim::jaro_winkler(str, user_arg))
                    .unwrap_or(0.0);
                name_similarity > 0.9 || nick_similarity > 0.9
            })
            .collect::<Vec<_>>();

        if name_matches.is_empty() {
            return Err(RemoveTracksFromError::NoUsersFound);
        } else if name_matches.len() > 1 {
            return Err(RemoveTracksFromError::MultipleUsersFound);
        }
        let user_id = name_matches
            .first()
            .expect("There should be exactly one member in the vector")
            .user
            .id;

        let tracks_removed_count = self.playback_queue.remove_tracks_from_user(user_id);

        self.push_current_state_to_undo_stack().await;
        Ok(tracks_removed_count)
    }
    pub fn remove_tracks_in_range<R>(&mut self, range: R) -> usize
    where
        R: RangeBounds<usize> + Debug,
    {
        self.playback_queue.remove_tracks_in_range(range)
    }
}

#[derive(Debug, Error)]
pub enum RemoveTracksFromError {
    #[error("Could not get the server member list")]
    ErrorFetchingMembers,

    #[error("Username argument yielded no results")]
    NoUsersFound,

    #[error("Username argument yielded more than one potential user")]
    MultipleUsersFound,
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
