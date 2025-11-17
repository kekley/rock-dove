use std::{path::PathBuf, process::Stdio};

use thiserror::Error;
use tokio::process::Command;
use tracing::{Level, event};

use crate::yt_dlp::{
    VideoQuery, YtDlp,
    args::{PLAYLIST_SEARCH_ARGS, VIDEO_SEARCH_ARGS, VIDEO_STREAM_SEARCH_ARGS},
    playlist::VideoInfo,
    video::VideoStreamInfo,
};

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("IOError when executing command: {0}")]
    IOError(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct YtDlpSidecar {
    executable: PathBuf,
}

impl YtDlpSidecar {
    pub fn new(executable_path: &str) -> Self {
        let path_buf = PathBuf::from(executable_path);
        Self {
            executable: path_buf,
        }
    }
}

impl YtDlp for YtDlpSidecar {
    async fn search_for_video(
        &self,
        query: &super::VideoQuery,
    ) -> Result<super::playlist::VideoInfo, super::YtDlpError> {
        let query_arg = match query {
            VideoQuery::Url(str) => str,
            VideoQuery::SearchTerm(str) => {
                #[cfg(feature = "tracing")]
                event!(Level::WARN, "Searching for playlists from a yt search");
                &format!("ytsearch:{str}")
            }
        };

        let mut command = Command::new(&self.executable);
        command.args(VIDEO_SEARCH_ARGS);
        command.args(std::iter::once(query_arg));

        command.stdout(Stdio::piped());

        let child = command.spawn()?;

        let output = child.wait_with_output().await?;

        let str = str::from_utf8(&output.stdout)?;

        let video_info = serde_json::from_str::<VideoInfo>(str)?;

        Ok(video_info)
    }

    async fn search_for_playlist(
        &self,
        url: &str,
    ) -> Result<Vec<super::playlist::VideoInfo>, super::YtDlpError> {
        let mut command = Command::new(&self.executable);
        command.args(PLAYLIST_SEARCH_ARGS);
        command.args(std::iter::once(url));

        command.stdout(Stdio::piped());

        let child = command.spawn()?;

        let output = child.wait_with_output().await?;

        let str = str::from_utf8(&output.stdout)?;

        Ok(str
            .lines()
            .filter_map(|line| match serde_json::from_str::<VideoInfo>(line) {
                Ok(vid) => Some(vid),
                Err(err) => {
                    event!(Level::ERROR, "{err}");
                    None
                }
            })
            .collect::<Vec<_>>())
    }

    async fn get_audio_streams(
        &self,
        video: &VideoInfo,
    ) -> Result<super::video::VideoStreamInfo, super::YtDlpError> {
        let mut command = Command::new(&self.executable);
        command.args(VIDEO_STREAM_SEARCH_ARGS);
        command.args(std::iter::once(video.url()));

        command.stdout(Stdio::piped());

        let child = command.spawn()?;

        let output = child.wait_with_output().await?;

        let str = str::from_utf8(&output.stdout)?;

        Ok(serde_json::from_str::<VideoStreamInfo>(str)?)
    }
}

#[cfg(test)]
mod test {

    use std::env::current_dir;

    use crate::yt_dlp::{YtDlp, sidecar::YtDlpSidecar};

    #[tokio::test]
    pub async fn search_video() {
        let cwd = current_dir().unwrap();
        println!("cwd: {cwd:?}");
        let sidecar = YtDlpSidecar::new("./binaries/yt-dlp_linux");
        let result = sidecar
            .search_for_video(&crate::yt_dlp::VideoQuery::SearchTerm(
                "silly cats".to_string(),
            ))
            .await
            .unwrap();
        println!("{result:?}");
    }
}
