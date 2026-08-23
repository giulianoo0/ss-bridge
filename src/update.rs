use std::sync::OnceLock;

use serde::Deserialize;

const LATEST: &str = "https://api.github.com/repos/giulianoo0/ss-bridge/releases/latest";
pub const RELEASES: &str = "https://github.com/giulianoo0/ss-bridge/releases/latest";

static AVAILABLE: OnceLock<String> = OnceLock::new();

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

pub fn available() -> Option<&'static str> {
    AVAILABLE.get().map(String::as_str)
}

pub async fn check() {
    let Ok(latest) = fetch_latest().await else { return };
    if newer(&latest, env!("CARGO_PKG_VERSION")) {
        let _ = AVAILABLE.set(latest);
    }
}

async fn fetch_latest() -> anyhow::Result<String> {
    let release: Release = reqwest::Client::new()
        .get(LATEST)
        .header("User-Agent", concat!("ss-bridge/", env!("CARGO_PKG_VERSION")))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(release.tag_name.trim_start_matches('v').to_string())
}

fn newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(candidate) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn compares_numerically() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("0.1.10", "0.1.9"));
        assert!(!newer("0.1.5", "0.1.5"));
        assert!(!newer("0.1.4", "0.1.5"));
    }
}
