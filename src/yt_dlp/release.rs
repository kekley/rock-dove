use core::from;
use std::{collections::HashMap, fmt, marker::PhantomData, sync::Arc};

use reqwest::{
    Client,
    header::{self, HeaderMap, HeaderName, HeaderValue, IntoHeaderName, InvalidHeaderValue},
};
use serde::Deserialize;
use thiserror::Error;

use crate::yt_dlp::download::{
    DownloadError,
    fetcher::{Fetchable, Fetcher},
};

pub struct ReleaseFetcher<'a> {
    headers: HeaderMap,
    client: Client,
    phantom: &'a PhantomData<()>,
}

#[derive(Debug, Error)]
pub enum FetcherError {
    #[error("Reqwest Error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Invalid header value. Error: {0}")]
    HeaderError(#[from] InvalidHeaderValue),
}

impl ReleaseFetcher<'_> {
    const USER_AGENT: &'static str = "user-agent";
    const USER_AGENT_VAL: &'static str = "my-release-fetcher-app";
    const ACCEPT: &'static str = "Accept";
    const ACCEPT_VAL: &'static str = "application/vnd.github+json";
    const AUTHORIZATION: &'static str = "Authorization";
    const API_VER_KEY: &'static str = "X-GitHub-Api-Version";
    const API_VER_VAL: &'static str = "2022-11-28";
}

impl<'a> Fetcher for ReleaseFetcher<'a> {
    type K = &'static str;
    type V = &'a str;
    type OutputType = Release;
    type Error = FetcherError;

    fn new(
        url: &str,
        timeout: std::time::Duration,
        header_args: impl Iterator<Item = (Self::K, Self::V)>,
    ) -> Result<Self, Self::Error> {
        let mut headers = HeaderMap::new();
        for arg in header_args {
            let header_value = HeaderValue::from_str(arg.1)?;
            headers.insert(arg.0, header_value);
        }

        let client = Client::builder().timeout(timeout).build()?;

        Ok(Self {
            headers,
            client,
            phantom: &PhantomData,
        })
    }
}

#[derive(Debug, Clone)]
pub enum Architecture {
    X86,
    X86_64,
    Arm,
    Aarch64,
    Other(String),
}

#[derive(Debug, Clone)]
pub enum Platform {
    Windows,
    Linux,
    Mac,
    Other(String),
}

impl Platform {
    pub fn detect() -> Self {
        let os = std::env::consts::OS;

        match os {
            "windows" => Platform::Windows,
            "linux" => Platform::Linux,
            "macos" => Platform::Mac,
            _ => Platform::Other(os.to_string()),
        }
    }
}

impl Architecture {
    pub fn detect() -> Self {
        let arch = std::env::consts::ARCH;

        match arch {
            "x86_64" => Architecture::X86_64,
            "x86" => Architecture::X86,
            "armv7l" => Architecture::Arm,
            "aarch64" => Architecture::Aarch64,
            _ => Architecture::Other(arch.to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn get_release_for<'a>(
        &'a self,
        architecture: &Architecture,
        platform: &Platform,
    ) -> Option<&'a Asset> {
        const BASE_ASSET_NAME: &str = "yt-dlp";
        self.assets.iter().find(|asset| {
            let name = &asset.name;

            match (platform, architecture) {
                (Platform::Windows, Architecture::X86_64) => {
                    name.contains(&format!("{}.exe", BASE_ASSET_NAME))
                }
                (Platform::Windows, Architecture::X86) => {
                    name.contains(&format!("{}_x86.exe", BASE_ASSET_NAME))
                }

                (Platform::Linux, Architecture::X86_64) => {
                    name.contains(&format!("{}_linux", BASE_ASSET_NAME))
                }
                (Platform::Linux, Architecture::Arm) => {
                    name.contains(&format!("{}_linux_armv7l", BASE_ASSET_NAME))
                }
                (Platform::Linux, Architecture::Aarch64) => {
                    name.contains(&format!("{}_linux_aarch64", BASE_ASSET_NAME))
                }

                (Platform::Mac, _) => name.contains(&format!("{}_macos", BASE_ASSET_NAME)),

                _ => false,
            }
        })
    }
}

impl Fetchable for Release {
    fn from_body(body: &[u8]) -> Result<Self, DownloadError> {
        Ok(serde_json::from_slice::<Release>(body)?)
    }
}

impl fmt::Display for Release {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Release: tag={}, assets={};",
            self.tag_name,
            self.assets.len()
        )
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub download_url: String,
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Asset: name={}, url={};", self.name, self.download_url)
    }
}

struct AssetFile {
    data: Vec<u8>,
}

impl Fetchable for AssetFile {
    fn from_body(body: &[u8]) -> Result<Self, DownloadError> {
        todo!()
    }
}

impl Asset {
    pub fn to_fetcher(&self) {
        todo!()
    }
}
