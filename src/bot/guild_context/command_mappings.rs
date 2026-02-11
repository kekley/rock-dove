use std::sync::{Arc, Weak};

use hashbrown::HashMap;

use crate::bot::command::Command;

pub struct CommandAliases {
    aliases: Box<[Arc<str>]>,
    mappings: HashMap<Weak<str>, Command>,
    reverse_mappings: HashMap<Weak<str>, Command>,
}

impl Default for CommandAliases {
    fn default() -> Self {
        Self {
            aliases: todo!(),
            mappings: todo!(),
            reverse_mappings: todo!(),
        }
    }
}

impl CommandAliases {
    pub fn get_command_aliases(&self) -> impl Iterator<Item = &str> {
        //TODO
        unimplemented!();
        std::iter::empty()
    }
    pub fn set_command_alias(&mut self, alias: &str, command: Command) -> Result<(), ()> {
        todo!();
    }
    pub fn get_command_for_alias(&self, alias: &str) -> Option<Command> {
        todo!()
    }
    pub fn get_alias_for_command(&self, command: Command) -> Option<&str> {
        todo!()
    }
}
