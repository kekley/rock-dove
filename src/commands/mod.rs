use std::path::PathBuf;

use thiserror::Error;
use tokio::process::Child;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("IOError when executing command: {0}")]
    IOError(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct Command {
    executable: PathBuf,
    args: Vec<String>,
}

impl Command {
    pub fn new(executable_path: &str) -> Self {
        let path_buf = PathBuf::from(executable_path);
        Self {
            executable: path_buf,
            args: vec![],
        }
    }

    pub fn append_arg<S: AsRef<str>>(&mut self, arg: S) {
        self.args.push(arg.as_ref().to_string());
    }

    pub fn append_arg_many<I: IntoIterator<Item = S>, S: AsRef<str>>(&mut self, args: I) {
        self.args
            .extend(args.into_iter().map(|s| s.as_ref().to_string()));
    }

    pub fn execute(self) -> Result<Child, CommandError> {
        let mut command = tokio::process::Command::new(&self.executable);
        command.args(&self.args);
        Ok(command.spawn()?)
    }
}

#[cfg(test)]
mod test {

    use std::env::current_dir;

    use serde_json::Value;

    use crate::commands::Command;
    #[tokio::test]
    pub async fn search_video() {
        let cwd = current_dir().unwrap();
        println!("cwd: {cwd:?}");
        let mut command = Command::new("./binaries/yt-dlp_linux");
        let args = ["--no-progress", "--dump-json", "ytsearch: silly cats"];
        command.append_arg_many(args);
        let result = command.execute().unwrap();
        let a = result.wait_with_output().await.unwrap();
    }
}
