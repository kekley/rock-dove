use std::{fmt, io::Cursor, time::Duration};

use reqwest::header::InvalidHeaderValue;
use serde::Deserialize;
use thiserror::Error;

use crate::git::{
    binary::BinaryFile,
    fetcher::{GithubFetchable, GithubFetcher},
};

#[derive(Debug, Error)]
pub enum FetcherError {
    #[error("Reqwest Error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Invalid header value. Error: {0}")]
    HeaderError(#[from] InvalidHeaderValue),
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

#[derive(Debug, Error)]
pub(crate) enum ReleaseDecodeError {
    #[error("Json Error: {0}")]
    JsonError(#[from] serde_json::Error),
}

impl GithubFetchable for Release {
    fn from_body(body: Vec<u8>) -> Result<Self, Self::Error> {
        let reader = Cursor::new(body);
        Ok(serde_json::from_reader(reader)?)
    }

    type Error = ReleaseDecodeError;
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
impl Asset {
    pub(crate) fn to_fetcher(&self, auth: Option<&str>) -> GithubFetcher<BinaryFile> {
        let url = &self.download_url;
        GithubFetcher::new(url, Duration::from_secs(5), auth)
    }
}
