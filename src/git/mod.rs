use reqwest::{Client, Url};

use crate::yt_dlp::sidecar::YtDlpSidecar;

pub async fn fetch_ytlp_release(
    _client: &Client,
    _url: &Url,
    _sidecar: &mut YtDlpSidecar,
) -> Result<(), ()> {
    todo!()
}
