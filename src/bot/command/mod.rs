use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

pub mod execute;
pub mod parse;
pub mod remove;

#[derive(
    Debug, EnumIter, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum Command {
    Help,
    Leave,
    Join,
    Next,
    Back,
    List,
    Shuffle,
    Play,
    Add,
    Clear,
    Loop,
    Remove,
    Pause,
    Resume,
    NowPlaying,
    Undo,
    Redo,
    Mute,
    Unmute,
    Beep,
    Ffmpeg,
    Prefix,
    Alias,
}

impl Command {
    pub const fn get_default_alias(self) -> &'static str {
        match self {
            Command::Help => "help",
            Command::Leave => "leave",
            Command::Join => "join",
            Command::Next => "next",
            Command::Back => "back",
            Command::List => "list",
            Command::Shuffle => "shuffle",
            Command::Play => "play",
            Command::Add => "add",
            Command::Clear => "clear",
            Command::Loop => "loop",
            Command::Remove => "remove",
            Command::Pause => "pause",
            Command::Resume => "resume",
            Command::NowPlaying => "nowplaying",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Mute => "mute",
            Command::Unmute => "unmute",
            Command::Beep => "beep",
            Command::Ffmpeg => "ffmpreg",
            Command::Alias => "alias",
            Command::Prefix => "prefix",
        }
    }
    ///Get the syntax for using the command
    pub const fn syntax(self) -> &'static str {
        match self {
            Command::Help => "",
            Command::Leave => "",
            Command::Join => "",
            Command::Next => "",
            Command::Back => "",
            Command::List => "",
            Command::Shuffle => "",
            Command::Play => "{ url | search_text }",
            Command::Add => "{ url | playlist_url | search_text }",
            Command::Clear => "",
            Command::Loop => "{ off | single | queue }",
            Command::Remove => "{  at | past | until | from }",
            Command::Pause => "",
            Command::Resume => "",
            Command::NowPlaying => "",
            Command::Undo => "",
            Command::Redo => "",
            Command::Mute => "",
            Command::Unmute => "",
            Command::Beep => "",
            Command::Ffmpeg => "{ url }",
            Command::Alias => "{ current_command new_command }",
            Command::Prefix => "{ new_prefix }",
        }
    }
    ///A verbal description of the command
    pub const fn description(self) -> &'static str {
        match self {
            Command::Help => "Show this list.",
            Command::Leave => "Remove the bot from any voice channels.",
            Command::Join => "Join the voice channel you're in.",
            Command::Next => "Start playing the next track",
            Command::List => "List the current contents of the queue.",
            Command::Shuffle => "Shuffle the contents of the queue.",
            Command::Play => "Bypass the queue and play a song from a url or youtube search",
            Command::Add => "Add a song or playlist to the queue from a url or youtube search.",
            Command::Clear => "Clear the queue.",
            Command::Loop => {
                "Set the loop mode.\n\toff = No looping\n\tsingle = Loop the current song indefinitely\n\tqueue = Loop the queue when it ends"
            }
            Command::Remove => {
                "Remove one or more tracks from the queue.\n\tat (track position) = Remove the track at (track position)\n\tpast (track position) = Remove all tracks after (track position)\n\tuntil (track position) = Remove all tracks up to (track position)\n\tfrom (username) = Remove all tracks added by (username)"
            }
            Command::Pause => "Pause the current track.",
            Command::Resume => "Resume the current track.",
            Command::NowPlaying => "See the name of the current track.",
            Command::Undo => "Undo the last change made to the queue.",
            Command::Redo => "Undo the last undo..?",
            Command::Mute => "Mute the bot",
            Command::Unmute => "Unmute",
            Command::Beep => "Say hi",
            Command::Ffmpeg => "Play back a raw audio stream from the web",
            Command::Alias => "Change the name of a command",
            Command::Back => "Go back to the last track",
            Command::Prefix => "Change the prefix that comes before a command",
        }
    }
}
