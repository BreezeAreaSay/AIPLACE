//! Self-update from GitHub Releases (`triad update`) and a quiet startup version check.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const REPO_OWNER: &str = "BreezeAreaSay";
pub const REPO_NAME: &str = "AIPLACE";
pub const BIN_NAME: &str = "triad";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub url: String,
}

/// Blocking — call before the tokio runtime starts.
pub fn self_update(check_only: bool) -> Result<()> {
    let target = self_update::get_target();
    let bin_file = if cfg!(windows) { format!("{BIN_NAME}.exe") } else { BIN_NAME.to_string() };
    // Archives are laid out as `triad-<ver>-<target>/triad[.exe]` (see release.yml).
    let in_archive = format!("{BIN_NAME}-{{{{ version }}}}-{{{{ target }}}}/{bin_file}");
    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(&bin_file)
        .bin_path_in_archive(&in_archive)
        .target(target)
        .current_version(VERSION)
        .show_download_progress(true)
        .no_confirm(true)
        .build()
        .context("cannot configure updater")?;
    let latest = status.get_latest_release().context("cannot reach GitHub Releases")?;
    let newer = self_update::version::bump_is_greater(VERSION, latest.version.as_str()).unwrap_or(false);
    if !newer {
        println!("✔ Уже последняя версия: v{VERSION}");
        return Ok(());
    }
    println!("→ Доступна v{} (у тебя v{VERSION}), цель: {target}", latest.version);
    if check_only {
        println!("  Обновить: triad update");
        return Ok(());
    }
    let has_asset = latest.assets.iter().any(|a| a.name.contains(target));
    if !has_asset {
        anyhow::bail!(
            "в релизе v{} нет сборки под {target}. Собери из исходников: cargo install --git https://github.com/{REPO_OWNER}/{REPO_NAME} --root ~/.triad triad-canvas-backend",
            latest.version
        );
    }
    let result = status.update().context("update failed")?;
    println!("✔ Обновлено до v{}. Перезапусти: triad", result.version());
    Ok(())
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

/// Non-blocking check used at startup; failures are swallowed (offline is fine).
pub async fn check_latest() -> Option<UpdateInfo> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .user_agent(format!("triad-canvas/{VERSION}"))
        .build()
        .ok()?;
    let url = format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let rel: Release = client.get(url).send().await.ok()?.error_for_status().ok()?.json().await.ok()?;
    let latest = rel.tag_name.trim_start_matches('v').to_string();
    if self_update::version::bump_is_greater(VERSION, &latest).unwrap_or(false) {
        Some(UpdateInfo { current: VERSION.into(), latest, url: rel.html_url })
    } else {
        None
    }
}
