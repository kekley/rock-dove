use compact_str::CompactString;
use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use tracing::{Level, event};

use crate::bot::command::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandAliases {
    mappings: HashMap<CompactString, Command>,
    reverse_mappings: HashMap<Command, CompactString>,
}

impl Default for CommandAliases {
    fn default() -> Self {
        let mappings = Command::iter()
            .map(|c| (CompactString::from(c.get_default_alias()), c))
            .collect();
        let reverse_mappings = Command::iter()
            .map(|c| (c, CompactString::from(c.get_default_alias())))
            .collect();

        Self {
            mappings,
            reverse_mappings,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SetAliasError {
    #[error("No new command was specified")]
    EmptyAlias,
    #[error("Commands can only contain ascii")]
    NotAscii,
    #[error("That command is already in use")]
    CommandConflict,
    #[error("You can't remap the help command")]
    NotAllowed,
    #[error("The max length for a command is 24 characters")]
    TooLong,
    #[error("there's a bug in the command mapping oops")]
    InternalError,
}

impl CommandAliases {
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.mappings.keys().map(CompactString::as_str)
    }
    pub fn set_command_alias(
        &mut self,
        alias: &str,
        command: Command,
    ) -> Result<(), SetAliasError> {
        if command == Command::Help {
            return Err(SetAliasError::NotAllowed);
        }
        let trimmed_lowercase_alias = {
            let trimmed = alias.trim();
            if trimmed.is_empty() {
                return Err(SetAliasError::EmptyAlias);
            }
            if trimmed.len() > 24 {
                return Err(SetAliasError::TooLong);
            }
            if !trimmed.is_ascii() {
                return Err(SetAliasError::NotAscii);
            }

            let mut lowercase = CompactString::from(trimmed);
            lowercase.make_ascii_lowercase();
            lowercase
        };
        if self.mappings.contains_key(trimmed_lowercase_alias.as_str()) {
            return Err(SetAliasError::CommandConflict);
        }
        let Some(old_mapping) = self.reverse_mappings.remove(&command) else {
            event!(
                Level::ERROR,
                "The reverse_mappings map did not contain a command"
            );
            return Err(SetAliasError::InternalError);
        };
        if self.mappings.remove(&old_mapping).is_none() {
            event!(Level::ERROR, "The mappings map did not contain a command");
            return Err(SetAliasError::InternalError);
        }
        self.mappings
            .insert(trimmed_lowercase_alias.clone(), command);
        self.reverse_mappings
            .insert(command, trimmed_lowercase_alias);

        Ok(())
    }
    pub fn get_command_for_alias(&self, alias: &str) -> Option<Command> {
        let trimmed_lowercase_alias = {
            let trimmed = alias.trim();
            let mut lowercase = CompactString::from(trimmed);
            lowercase.make_ascii_lowercase();
            lowercase
        };
        self.mappings.get(&trimmed_lowercase_alias).copied()
    }
    pub fn get_alias_for_command(&self, command: Command) -> Option<&str> {
        self.reverse_mappings
            .get(&command)
            .map(CompactString::as_str)
    }
}
