use std::marker::PhantomData;

use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum ReleaseFetchError {
    #[error("Error fetching from url")]
    HTTPRequestError(String),
    #[error("Response was not valid json. Error: {0}")]
    InvalidResponse(#[from] serde_json::Error),
}

pub(crate) trait GithubFetchable: Sized {
    type Error;

    fn from_body(body: Vec<u8>) -> Result<Self, Self::Error>;
}

pub(crate) struct GithubFetcher<T: GithubFetchable> {
    url: String,
    client: Client,
    headers: HeaderMap,
    phantom: PhantomData<T>,
}

impl<T: GithubFetchable> GithubFetcher<T> {
    pub(crate) fn new(url: &str, timeout: std::time::Duration, auth: Option<&str>) -> Self {
        const USER_AGENT: &str = "user-agent";
        const USER_AGENT_VAL: &str = "my-release-fetcher-app";
        const ACCEPT: &str = "Accept";
        const ACCEPT_VAL: &str = "application/vnd.github+json";
        const AUTHORIZATION: &str = "Authorization";
        const API_VER_KEY: &str = "X-GitHub-Api-Version";
        const API_VER_VAL: &str = "2022-11-28";

        const DEFAULT_HEADERS: [(&str, &str); 3] = [
            (USER_AGENT, USER_AGENT_VAL),
            (ACCEPT, ACCEPT_VAL),
            (API_VER_KEY, API_VER_VAL),
        ];
        let mut headers = HeaderMap::new();

        for (k, v) in DEFAULT_HEADERS {
            headers.insert(k, HeaderValue::from_static(v));
        }
        if let Some(token) = auth {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))
                    .expect("Auth token should be valid for placement in http header"),
            );
        };

        let client = Client::builder().timeout(timeout).build().unwrap();

        Self {
            url: url.to_string(),
            client,
            headers,
            phantom: PhantomData,
        }
    }
}
