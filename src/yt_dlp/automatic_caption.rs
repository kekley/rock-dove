use std::fmt;
use std::hash::Hasher;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomaticCaption {
    #[serde(rename = "ext")]
    pub extension: Extension,
    pub url: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extension {
    Json3,
    Srv1,
    Srv2,
    Srv3,
    Ttml,
    Vtt,
}

impl fmt::Display for AutomaticCaption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Caption(lang={}, ext={:?})",
            self.name.as_deref().unwrap_or("unknown"),
            self.extension
        )
    }
}

impl Eq for AutomaticCaption {}

impl std::hash::Hash for AutomaticCaption {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.url.hash(state);
        self.name.hash(state);
        std::mem::discriminant(&self.extension).hash(state);
    }
}

impl fmt::Display for Extension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Extension::Json3 => write!(f, "json3"),
            Extension::Srv1 => write!(f, "srv1"),
            Extension::Srv2 => write!(f, "srv2"),
            Extension::Srv3 => write!(f, "srv3"),
            Extension::Ttml => write!(f, "ttml"),
            Extension::Vtt => write!(f, "vtt"),
        }
    }
}

impl Eq for Extension {}

impl std::hash::Hash for Extension {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
    }
}
