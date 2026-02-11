use strum_macros::EnumIter;

pub mod execute;
pub mod parse;
pub mod remove;

#[derive(Debug, EnumIter, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    Alias,
    Save,
    Load,
}

impl Command {
    ///Get the syntax for using the command
    pub fn syntax(self) -> &'static str {
        match self {
            Command::Help => "",
            Command::Leave => "",
            Command::Join => "",
            Command::Next => "",
            Command::List => "",
            Command::Shuffle => "",
            Command::Play => "{ url | playlist url | search text }",
            Command::Add => "{ url | playlist url | search text }",
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
            Command::Alias => "",
            Command::Back => "",
            Command::Save => todo!(),
            Command::Load => todo!(),
        }
    }
    ///A verbal description of the command
    pub fn description(self) -> &'static str {
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
                "Set the loop mode.\noff = No looping\nsingle = Loop the current song indefinitely\nqueue = Loop the queue when it ends"
            }
            Command::Remove => {
                "Remove one or more tracks from the queue.\nremove at (track position) = Remove the track at (track position)\nremove past (track position) = Remove all tracks after (track position)\nremove until (track position) = Remove all tracks up to (track position)\nremove from (username) = Remove all tracks added by (username)"
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
            Command::Save => todo!(),
            Command::Load => todo!(),
        }
    }
}
