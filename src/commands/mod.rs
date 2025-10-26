use std::path::PathBuf;

use thiserror::Error;
use tokio::process::Child;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("IOError when executing command: {0}")]
    IOError(#[from] std::io::Error),
}

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

    pub fn append_arg(&mut self, arg: &str) {
        self.args.push(arg.to_string());
    }

    pub fn append_arg_many<'a>(&mut self, args: impl std::iter::Iterator<Item = &'a str>) {
        self.args.extend(args.map(|str| str.to_string()));
    }

    pub fn execute(&self) -> Result<Child, CommandError> {
        let mut command = tokio::process::Command::new(&self.executable);
        command.args(&self.args);
        Ok(command.spawn()?)
    }
}
