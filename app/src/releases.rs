//! Firmware releases from the project's GitHub page, and verified downloads.
//!
//! Each release (made by .github/workflows/build.yml on a version tag) carries
//! `rk71_rgb_<tag>.bin` (the VIA keymap firmware) and `SHA256SUMS.txt`. A download is only used
//! when its SHA-256 matches that list. The stock firmware is published as the `stock-v1.12`
//! release (`rk71_fw_v1.12.bin`).

use std::io::Read as _;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const REPO: &str = "josemanuque/rk71-qmk-port";
const SUMS_FILE: &str = "SHA256SUMS.txt";

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    /// Test builds (tags ending in b, -beta, -rc) are marked pre-release on GitHub.
    pub prerelease: bool,
    /// The original Royal Kludge firmware rather than this project's.
    pub stock: bool,
    /// YYYY-MM-DD
    pub date: String,
    /// The release notes (Markdown), as written on GitHub.
    pub notes: String,
    firmware: Asset,
    sums: Option<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    prerelease: bool,
    draft: bool,
    published_at: Option<String>,
    body: Option<String>,
    assets: Vec<Asset>,
}

impl Release {
    /// Version as the firmware reports it (0x00MMmmpp), from a tag like v0.7b or v0.5.5b.
    pub fn version(&self) -> Option<u32> {
        parse_version(&self.tag)
    }

    pub fn file_name(&self) -> &str {
        &self.firmware.name
    }

    /// Download the firmware and check it against the release's SHA-256 list.
    pub fn download(&self) -> Result<Vec<u8>> {
        let sums = self.sums.as_ref().ok_or_else(|| anyhow!("{} has no {SUMS_FILE}; not installing an unverified file", self.tag))?;
        let list = String::from_utf8(get_bytes(&sums.browser_download_url)?).context("unreadable checksum list")?;
        let expected = list
            .lines()
            .filter_map(|l| l.split_once(char::is_whitespace))
            .find(|(_, name)| name.trim().trim_start_matches('*') == self.firmware.name)
            .map(|(hash, _)| hash.to_ascii_lowercase())
            .ok_or_else(|| anyhow!("{} is not in the release's checksum list", self.firmware.name))?;
        let image = get_bytes(&self.firmware.browser_download_url)?;
        let actual = hex(&Sha256::digest(&image));
        if actual != expected {
            bail!("the download does not match its checksum (got {actual}); not installing it");
        }
        Ok(image)
    }
}

/// Every release with a firmware image, newest version first, the stock firmware last.
pub fn fetch() -> Result<Vec<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=50");
    let api: Vec<ApiRelease> = serde_json::from_slice(&get_bytes(&url)?).context("unexpected answer from GitHub")?;
    let mut list: Vec<Release> = api
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let stock = r.tag_name.starts_with("stock");
            let firmware = r
                .assets
                .iter()
                .find(|a| a.name.ends_with(".bin") && !a.name.contains("default-keymap"))?
                .clone();
            let sums = r.assets.iter().find(|a| a.name == SUMS_FILE).cloned();
            Some(Release {
                date: r.published_at.as_deref().unwrap_or("").chars().take(10).collect(),
                notes: r.body.unwrap_or_default().trim().to_string(),
                prerelease: r.prerelease,
                stock,
                tag: r.tag_name,
                firmware,
                sums,
            })
        })
        .collect();
    list.sort_by_key(|r| (r.stock, std::cmp::Reverse(r.version())));
    Ok(list)
}

/// "v0.7b" -> 0x00000700, "v0.5.5b" -> 0x00000505. None for tags without a version.
pub fn parse_version(tag: &str) -> Option<u32> {
    let digits: Vec<u32> = tag
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .take_while(|s| !s.is_empty())
        .map(|s| s.parse().ok())
        .collect::<Option<_>>()?;
    if digits.is_empty() || tag.starts_with("stock") {
        return None;
    }
    let part = |i: usize| digits.get(i).copied().unwrap_or(0).min(255);
    Some(part(0) << 16 | part(1) << 8 | part(2))
}

/// 0x00000700 -> "0.7", 0x00000505 -> "0.5.5"
pub fn format_version(v: u32) -> String {
    let (major, minor, patch) = (v >> 16 & 0xFF, v >> 8 & 0xFF, v & 0xFF);
    if patch == 0 { format!("{major}.{minor}") } else { format!("{major}.{minor}.{patch}") }
}

fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build();
    let response = agent
        .get(url)
        .set("User-Agent", "rk71-configurator")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| anyhow!("could not reach GitHub: {e}"))?;
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes).context("download interrupted")?;
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_from_tags() {
        assert_eq!(parse_version("v0.7b"), Some(0x0700));
        assert_eq!(parse_version("v0.5.5b"), Some(0x0505));
        assert_eq!(parse_version("v1.0.0"), Some(0x010000));
        assert_eq!(parse_version("v0.7.1-rc1"), Some(0x0701));
        assert_eq!(parse_version("stock-v1.12"), None);
        assert_eq!(format_version(0x0505), "0.5.5");
        assert_eq!(format_version(0x0700), "0.7");
    }
}
