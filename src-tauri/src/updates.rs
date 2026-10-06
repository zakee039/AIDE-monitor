use crate::model::ApiError;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const RELEASE_URL: &str = "https://github.com/zakee039/AIDE-monitor/releases/latest";
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current_version: String,
    latest_version: String,
    update_available: bool,
    notes: String,
}

fn version(tag: &str) -> Result<Version, ApiError> {
    Version::parse(tag.trim().trim_start_matches('v'))
        .map_err(|_| ApiError::new("NETWORK_ERROR", "GitHub 版本信息无效"))
}

pub async fn check() -> Result<UpdateInfo, ApiError> {
    let failed = || ApiError::new("NETWORK_ERROR", "无法检查 GitHub 更新，请稍后重试");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| failed())?;
    let mut response = client
        .get("https://api.github.com/repos/zakee039/AIDE-monitor/releases/latest")
        .header(
            "User-Agent",
            concat!("AIDE-monitor/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| failed())?
        .error_for_status()
        .map_err(|_| failed())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| failed())? {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err(failed());
        }
        bytes.extend_from_slice(&chunk);
    }
    let release: Release = serde_json::from_slice(&bytes).map_err(|_| failed())?;
    if release.draft || release.prerelease {
        return Err(failed());
    }
    let latest = version(&release.tag_name)?;
    Ok(UpdateInfo {
        current_version: env!("CARGO_PKG_VERSION").into(),
        update_available: latest > version(env!("CARGO_PKG_VERSION"))?,
        latest_version: latest.to_string(),
        notes: release.body.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "queries the public GitHub Releases endpoint"]
    async fn check_public_release() {
        let result = check().await.unwrap();
        assert_eq!(result.current_version, env!("CARGO_PKG_VERSION"));
        assert!(version(&result.latest_version).is_ok());
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(version("v0.5.10").unwrap() > version("0.5.9").unwrap());
        assert!(version("0.5.1-beta.1").unwrap() < version("0.5.1").unwrap());
        assert!(version("invalid").is_err());
    }
}
