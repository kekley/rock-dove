use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct VideoInfo {
    title: String,
    webpage_url: String,
    channel: String,
    duration_string: String,
}

impl VideoInfo {
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn duration(&self) -> &str {
        &self.duration_string
    }
    pub fn url(&self) -> &str {
        &self.webpage_url
    }
}
