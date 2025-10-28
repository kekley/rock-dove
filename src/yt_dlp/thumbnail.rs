use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thumbnail {
    pub url: String,
    pub preference: i64,
    pub id: String,
    pub height: Option<i64>,
    pub width: Option<i64>,
    pub resolution: Option<String>,
}
impl fmt::Display for Thumbnail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Thumbnail(id={}, resolution={})",
            self.id,
            self.resolution.as_deref().unwrap_or("unknown")
        )
    }
}

impl Eq for Thumbnail {}

impl std::hash::Hash for Thumbnail {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.url.hash(state);
        self.preference.hash(state);
    }
}
