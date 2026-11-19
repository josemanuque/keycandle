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

pub const REPO: &str = "josemanuque/keycandle";
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
    /// The release on GitHub, for the app download and the full notes.
    pub page: String,
    firmware: Asset,
    sums: Option<Asset>,
    /// This computer's app build, when the release has one.
    app: Option<Asset>,
}

/// The app's file in each release, for the system this build runs on.
const APP_ASSET: &str = if cfg!(target_os = "windows") {
    "keycandle-windows-x64.exe"
} else if cfg!(target_os = "macos") {
    "keycandle-macos-universal.zip"
} else {
    "keycandle-linux-x64.tar.gz"
};

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
    #[serde(default)]
    html_url: String,
    assets: Vec<Asset>,
}

impl Release {
    /// Version as the firmware reports it (0x00MMmmpp), from a tag like v0.7b or v0.5.5b.
    pub fn version(&self) -> Option<u32> {
        parse_version(&self.tag)
    }

    /// Whether the release has this computer's build of the app.
    pub fn has_app(&self) -> bool {
        self.app.is_some()
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
            let app = r.assets.iter().find(|a| a.name == APP_ASSET).cloned();
            Some(Release {
                date: r.published_at.as_deref().unwrap_or("").chars().take(10).collect(),
                notes: r.body.unwrap_or_default().trim().to_string(),
                prerelease: r.prerelease,
                stock,
                tag: r.tag_name,
                page: r.html_url,
                firmware,
                sums,
                app,
            })
        })
        .collect();
    list.sort_by_key(|r| (r.stock, std::cmp::Reverse(r.version())));
    Ok(list)
}

/// The newest of this project's releases, if it's newer than `installed` (an update). A build newer
/// than every release (one of your own, or not released yet) gets nothing. `app`: only releases with
/// this computer's app build.
pub fn update_over(list: &[Release], installed: Option<u32>, app: bool) -> Option<Release> {
    let installed = installed?;
    let newest = list.iter().filter(|r| !r.stock && (!app || r.has_app())).max_by_key(|r| r.version().unwrap_or(0))?;
    newest.version().filter(|v| *v > installed).map(|_| newest.clone())
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
        .set("User-Agent", "keycandle")
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

    fn release(tag: &str, stock: bool) -> Release {
        let asset = |name: &str| Asset { name: name.into(), browser_download_url: String::new() };
        Release {
            tag: tag.into(),
            prerelease: true,
            stock,
            date: String::new(),
            notes: String::new(),
            page: String::new(),
            firmware: asset("fw.bin"),
            sums: None,
            app: Some(asset(APP_ASSET)),
        }
    }

    #[test]
    fn updates_only_when_a_release_is_newer() {
        let list = [release("v0.8.3b", false), release("v0.8.2b", false), release("stock-v1.12", true)];
        // Older installed: the newest release is offered.
        assert_eq!(update_over(&list, Some(0x0802), false).map(|r| r.tag), Some("v0.8.3b".into()));
        // The same, or a build newer than any release (unreleased): nothing.
        assert!(update_over(&list, Some(0x0803), false).is_none());
        assert!(update_over(&list, Some(0x0900), true).is_none());
        // Unknown installed version: nothing to compare with.
        assert!(update_over(&list, None, false).is_none());
    }

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
