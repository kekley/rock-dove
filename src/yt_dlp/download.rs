use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("Error fetching from url")]
    HTTPRequestError(String),
    #[error("Response was not valid json. Error: {0}")]
    InvalidResponse(#[from] serde_json::Error),
}

pub mod fetcher {

    use crate::yt_dlp::{
        download::DownloadError,
        release::{Asset, Release},
    };
    use std::{marker::PhantomData, sync::Arc, time::Duration};

    pub trait Fetchable: Sized {
        fn from_body(body: &[u8]) -> Result<Self, DownloadError>;
    }

    trait HeaderKey {}

    trait HeaderValue {}

    pub trait Fetcher: Sized {
        type K;
        type V;
        type Error;
        type OutputType: Fetchable;
        fn new(
            url: &str,
            timeout: Duration,
            header_args: impl Iterator<Item = (Self::K, Self::V)>,
        ) -> Result<Self, Self::Error>;
    }
}
