use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
};

use thiserror::Error;
use tokio::{process::Command, sync::RwLock};
use tracing::{Level, event};

use crate::yt_dlp::{
    SidecarUpdateError, VideoQuery, YtDlp, YtDlpError,
    args::{PLAYLIST_SEARCH_ARGS, UPDATE_ARGS, VIDEO_SEARCH_ARGS, VIDEO_STREAM_SEARCH_ARGS},
    playlist::VideoInfo,
    video::VideoStreamInfo,
};

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("IOError when executing command: {0}")]
    IOError(#[from] std::io::Error),
}

//TODO improve the auto update
#[derive(Debug)]
pub struct YtDlpSidecar {
    //kind of ugly but we use this lock to hold off requests if ytdlp is currently being updated
    executable_lock: RwLock<PathBuf>,
    cookies_path: Option<PathBuf>,
    //we set this to keep track of whether updating has not resolved the issue when getting videos
    search_failed_already: AtomicBool,
}

impl YtDlpSidecar {
    pub fn new(executable_path: &Path, cookies: Option<&Path>) -> Self {
        Self {
            cookies_path: cookies.map(|p| p.to_path_buf()),
            search_failed_already: AtomicBool::new(false),
            executable_lock: RwLock::new(executable_path.to_path_buf()),
        }
    }
    async fn update(&self) -> Result<(), SidecarUpdateError> {
        let executable_path_mut = self.executable_lock.write().await;

        let mut command = Command::new(executable_path_mut.as_path());
        command.args(UPDATE_ARGS);
        let mut update_proc = command.spawn()?;
        let exit_code = update_proc.wait().await?;

        if exit_code.success() {
            Ok(())
        } else {
            Err(SidecarUpdateError::ErrorCode(exit_code))
        }
    }
}

impl YtDlp for YtDlpSidecar {
    async fn search_for_video(
        &self,
        query: &super::VideoQuery,
    ) -> Result<super::playlist::VideoInfo, super::YtDlpError> {
        let executable_path = self.executable_lock.read().await;

        let query_arg = match query {
            VideoQuery::Url(str) => str,
            VideoQuery::SearchTerm(str) => {
                event!(Level::INFO, "yt search");
                &format!("ytsearch:{str}")
            }
        };

        let mut command = Command::new(executable_path.as_path());
        command.args(VIDEO_SEARCH_ARGS);
        if let Some(cookies_path) = self.cookies_path.as_ref() {
            command.args([std::ffi::OsStr::new("--cookies"), cookies_path.as_os_str()]);
        }
        command.args(std::iter::once(query_arg));

        command.stdout(Stdio::piped());

        let child = command.spawn()?;

        let mut output = child.wait_with_output().await?;

        let output_str = if output.status.success() {
            str::from_utf8(&output.stdout)?
        } else {
            //If we get an exit code signaling an error then presumably ytdlp failed to get anything from yt. We can try updating and then running the command again
            if !self.search_failed_already.load(Ordering::Acquire) {
                //should restructure this to use scopes tbh, but drop the read lock here because
                //`update` takes the write lock
                drop(executable_path);
                self.update().await?;

                let child = command.spawn()?;

                output = child.wait_with_output().await?;
                if output.status.success() {
                    str::from_utf8(&output.stdout)?
                } else {
                    self.search_failed_already.store(true, Ordering::Release);
                    return Err(YtDlpError::PostUpdateFailure);
                }
            } else {
                return Err(YtDlpError::PostUpdateFailure);
            }
        };
        Ok(serde_json::from_str::<VideoInfo>(output_str)?)
    }

    async fn search_for_playlist(
        &self,
        url: &str,
    ) -> Result<Vec<super::playlist::VideoInfo>, super::YtDlpError> {
        let executable_path = self.executable_lock.read().await;
        let mut command = Command::new(executable_path.as_path());
        command.args(PLAYLIST_SEARCH_ARGS);
        if let Some(cookies_path) = self.cookies_path.as_ref() {
            command.args([std::ffi::OsStr::new("--cookies"), cookies_path.as_os_str()]);
        }
        command.args(std::iter::once(url));

        command.stdout(Stdio::piped());

        let child = command.spawn()?;

        let mut output = child.wait_with_output().await?;
        let output_str = if output.status.success() {
            str::from_utf8(&output.stdout)?
        } else {
            //see `search_for_video` for an explanation on why this is like this
            if !self.search_failed_already.load(Ordering::Acquire) {
                drop(executable_path);
                self.update().await?;

                let child = command.spawn()?;

                output = child.wait_with_output().await?;
                if output.status.success() {
                    str::from_utf8(&output.stdout)?
                } else {
                    self.search_failed_already.store(true, Ordering::Release);
                    return Err(YtDlpError::PostUpdateFailure);
                }
            } else {
                return Err(YtDlpError::PostUpdateFailure);
            }
        };

        Ok(output_str
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
        let executable_path = self.executable_lock.read().await;
        let mut command = Command::new(executable_path.as_path());
        command.args(VIDEO_STREAM_SEARCH_ARGS);
        if let Some(cookies_path) = self.cookies_path.as_ref() {
            command.args([std::ffi::OsStr::new("--cookies"), cookies_path.as_os_str()]);
        }
        command.args(std::iter::once(video.url()));

        command.stdout(Stdio::piped());

        let child = command.spawn()?;
        let mut output = child.wait_with_output().await?;

        let output_str = if output.status.success() {
            str::from_utf8(&output.stdout)?
        } else {
            //see `search_for_video` for an explanation on why this is like this
            if !self.search_failed_already.load(Ordering::Acquire) {
                drop(executable_path);
                self.update().await?;

                let child = command.spawn()?;

                output = child.wait_with_output().await?;
                if output.status.success() {
                    str::from_utf8(&output.stdout)?
                } else {
                    self.search_failed_already.store(true, Ordering::Release);
                    return Err(YtDlpError::PostUpdateFailure);
                }
            } else {
                return Err(YtDlpError::PostUpdateFailure);
            }
        };
        Ok(serde_json::from_str::<VideoStreamInfo>(output_str)?)
    }
}

#[cfg(test)]
mod test {
    use crate::yt_dlp::{YtDlp, sidecar::YtDlpSidecar};
    use std::{env::current_dir, path::PathBuf};
    #[tokio::test]
    pub async fn search_video() {
        let cwd = current_dir().unwrap();
        println!("cwd: {cwd:?}");
        let sidecar = YtDlpSidecar::new(
            PathBuf::from("./binaries/yt-dlp_linux").as_path(),
            Some(PathBuf::from("./cookies.txt").as_path()),
        );
        let result = sidecar
            .search_for_video(&crate::yt_dlp::VideoQuery::SearchTerm(
                "silly cats".to_string(),
            ))
            .await
            .unwrap();
        println!("{result:?}");
    }
}
