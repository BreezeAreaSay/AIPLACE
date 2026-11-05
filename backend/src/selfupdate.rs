//! Update from source, right from the app. GitHub Releases are the preferred path
//! (`update.rs`), but while they are unavailable the app can update itself the way you do by
//! hand: `git pull`, rebuild the UI, rebuild the binary, restart. The heavy lifting runs in a
//! detached updater script (visible console) after the app exits, and the result lands in
//! `$TRIAD_HOME/update.status` for the next start to show.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::process::Command;

use crate::config;

#[derive(Debug, Clone, Serialize)]
pub struct SourceUpdate {
    pub repo_dir: String,
    pub branch: String,
    pub head: String,
    pub remote: String,
    pub behind: usize,
    pub ahead: usize,
    pub dirty: bool,
    pub commits: Vec<String>,
    pub mode: String,
    pub exe: String,
    pub checked_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub last: Option<String>,
    pub log_tail: String,
    pub running: bool,
}

fn looks_like_repo(p: &Path) -> bool {
    p.join(".git").exists() && p.join("frontend").is_dir() && p.join("backend").join("Cargo.toml").exists()
}

/// `TRIAD_REPO`, else walk up from the executable, else from the working directory.
pub fn find_repo_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("TRIAD_REPO") {
        let p = PathBuf::from(d);
        if looks_like_repo(&p) {
            return Some(p);
        }
    }
    let mut starts: Vec<PathBuf> = vec![];
    if let Ok(exe) = std::env::current_exe() {
        starts.push(exe);
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    for s in starts {
        let mut cur: Option<&Path> = Some(s.as_path());
        for _ in 0..8 {
            let Some(p) = cur else { break };
            if looks_like_repo(p) {
                return Some(p.to_path_buf());
            }
            cur = p.parent();
        }
    }
    None
}

/// desktop (Tauri shell) or cli (`triad serve`).
pub fn mode() -> &'static str {
    let name = std::env::current_exe().ok().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase())).unwrap_or_default();
    if name.contains("desktop") {
        "desktop"
    } else {
        "cli"
    }
}

async fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = tokio::time::timeout(Duration::from_secs(40), Command::new("git").args(args).current_dir(repo).output()).await.context("git timed out")??;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() {
        anyhow::bail!("git {} → {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(stdout)
}

/// Fetch and compare with `origin/<branch>`.
pub async fn check() -> Result<SourceUpdate> {
    let repo = find_repo_dir().context("репозиторий не найден: приложение запущено не из клона (задай TRIAD_REPO=путь к папке AIPLACE)")?;
    let branch = git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).await.unwrap_or_else(|_| "main".into());
    let branch = if branch == "HEAD" { "main".to_string() } else { branch };
    let fetch_err = git(&repo, &["fetch", "--quiet", "origin", &branch]).await.err().map(|e| e.to_string());
    let head = git(&repo, &["rev-parse", "--short=9", "HEAD"]).await.unwrap_or_default();
    let remote_ref = format!("origin/{branch}");
    let remote = git(&repo, &["rev-parse", "--short=9", &remote_ref]).await.unwrap_or_default();
    let counts = git(&repo, &["rev-list", "--left-right", "--count", &format!("HEAD...{remote_ref}")]).await.unwrap_or_else(|_| "0\t0".into());
    let mut it = counts.split_whitespace();
    let ahead: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let behind: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let commits = if behind > 0 {
        git(&repo, &["log", "--format=%h %s", "-n", "12", &format!("HEAD..{remote_ref}")]).await.unwrap_or_default().lines().map(str::to_string).collect()
    } else {
        vec![]
    };
    let dirty = !git(&repo, &["status", "--porcelain", "--untracked-files=no"]).await.unwrap_or_default().trim().is_empty();
    Ok(SourceUpdate {
        repo_dir: repo.display().to_string(),
        branch,
        head,
        remote,
        behind,
        ahead,
        dirty,
        commits,
        mode: mode().into(),
        exe: std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        checked_at: chrono::Utc::now().to_rfc3339(),
        error: fetch_err,
    })
}

#[cfg_attr(not(windows), allow(dead_code))]
const PS1: &str = r#"param([string]$Repo, [int]$ProcId, [string]$Mode, [string]$Exe, [string]$Home)
$ErrorActionPreference = "Stop"
$Log = Join-Path $Home "update.log"
$Status = Join-Path $Home "update.status"
Start-Transcript -Path $Log -Append | Out-Null
function Step($t) { Write-Host ""; Write-Host ("▸ " + $t) -ForegroundColor Green }
try {
  Set-Content $Status "running"
  Step "жду, пока закроется приложение (pid $ProcId)"
  while (Get-Process -Id $ProcId -ErrorAction SilentlyContinue) { Start-Sleep -Seconds 1 }
  Set-Location $Repo
  Step "git pull"
  git pull --ff-only; if ($LASTEXITCODE -ne 0) { throw "git pull не прошёл (локальные правки? выполни git stash)" }
  Step "frontend: npm install + build"
  Set-Location (Join-Path $Repo "frontend")
  npm install --no-audit --no-fund; if ($LASTEXITCODE -ne 0) { throw "npm install (frontend)" }
  npm run build; if ($LASTEXITCODE -ne 0) { throw "сборка фронтенда" }
  if ($Mode -eq "desktop") {
    Step "desktop: tauri build (несколько минут)"
    Set-Location (Join-Path $Repo "desktop")
    npm install --no-audit --no-fund; if ($LASTEXITCODE -ne 0) { throw "npm install (desktop)" }
    npx tauri build --no-bundle; if ($LASTEXITCODE -ne 0) { throw "сборка десктопа" }
    $Built = Join-Path $Repo "desktop\src-tauri\target\release\triad-desktop.exe"
  } else {
    Step "backend: cargo build --release"
    Set-Location (Join-Path $Repo "backend")
    cargo build --release; if ($LASTEXITCODE -ne 0) { throw "сборка бэкенда" }
    $Built = Join-Path $Repo "backend\target\release\triad.exe"
  }
  if ((Resolve-Path $Built).Path -ne (Resolve-Path $Exe -ErrorAction SilentlyContinue).Path) {
    Step "копирую новый бинарник в $Exe"
    Copy-Item $Built $Exe -Force
  }
  $Head = (git -C $Repo rev-parse --short=9 HEAD)
  Set-Content $Status ("ok " + $Head + " " + (Get-Date -Format s))
  Step "готово: $Head — запускаю"
  if ($Mode -eq "desktop") { Start-Process $Exe } else { Start-Process $Exe -ArgumentList "serve" }
  Start-Sleep -Seconds 2
} catch {
  Set-Content $Status ("fail " + $_.Exception.Message)
  Write-Host ""; Write-Host ("✖ " + $_.Exception.Message) -ForegroundColor Red
  Write-Host "Лог: $Log"
  if ($Mode -eq "desktop") { Start-Process $Exe } else { Start-Process $Exe -ArgumentList "serve" }
  Read-Host "Enter — закрыть окно"
} finally { Stop-Transcript | Out-Null }
"#;

#[cfg_attr(windows, allow(dead_code))]
const SH: &str = r#"#!/bin/sh
# usage: update.sh REPO PID MODE EXE HOME
REPO="$1"; PID="$2"; MODE="$3"; EXE="$4"; HOME_DIR="$5"
LOG="$HOME_DIR/update.log"; STATUS="$HOME_DIR/update.status"
exec >>"$LOG" 2>&1
echo "running" > "$STATUS"
step() { echo; echo "▸ $1"; }
fail() { echo "fail $1" > "$STATUS"; echo "✖ $1"; relaunch; exit 1; }
relaunch() { if [ "$MODE" = "desktop" ]; then nohup "$EXE" >/dev/null 2>&1 & else nohup "$EXE" serve >/dev/null 2>&1 & fi; }
step "жду выхода pid $PID"
while kill -0 "$PID" 2>/dev/null; do sleep 1; done
cd "$REPO" || fail "нет папки $REPO"
step "git pull"; git pull --ff-only || fail "git pull не прошёл (локальные правки?)"
step "frontend"; cd "$REPO/frontend" && npm install --no-audit --no-fund && npm run build || fail "сборка фронтенда"
if [ "$MODE" = "desktop" ]; then
  step "desktop"; cd "$REPO/desktop" && npm install --no-audit --no-fund && npx tauri build --no-bundle || fail "сборка десктопа"
  BUILT="$REPO/desktop/src-tauri/target/release/triad-desktop"
else
  step "backend"; cd "$REPO/backend" && cargo build --release || fail "сборка бэкенда"
  BUILT="$REPO/backend/target/release/triad"
fi
if [ "$(readlink -f "$BUILT")" != "$(readlink -f "$EXE")" ]; then step "копирую в $EXE"; cp -f "$BUILT" "$EXE" || fail "копирование"; fi
HEAD=$(git -C "$REPO" rev-parse --short=9 HEAD)
echo "ok $HEAD $(date -Iseconds)" > "$STATUS"
step "готово: $HEAD — запускаю"
relaunch
"#;

/// Spawn the detached updater and schedule our own exit. Returns the log path.
pub async fn run() -> Result<PathBuf> {
    let repo = find_repo_dir().context("репозиторий не найден (TRIAD_REPO)")?;
    let home = config::triad_home();
    std::fs::create_dir_all(&home).ok();
    let exe = std::env::current_exe().context("current exe")?;
    let pid = std::process::id().to_string();
    let log = home.join("update.log");
    std::fs::write(home.join("update.status"), "starting").ok();
    #[cfg(windows)]
    {
        let script = home.join("update.ps1");
        std::fs::write(&script, PS1).context("write update.ps1")?;
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .args(["-Repo"])
            .arg(&repo)
            .args(["-ProcId", &pid, "-Mode", mode(), "-Exe"])
            .arg(&exe)
            .arg("-Home")
            .arg(&home)
            .spawn()
            .context("cannot start powershell updater")?;
    }
    #[cfg(not(windows))]
    {
        let script = home.join("update.sh");
        std::fs::write(&script, SH).context("write update.sh")?;
        std::process::Command::new("sh")
            .arg(&script)
            .arg(&repo)
            .arg(&pid)
            .arg(mode())
            .arg(&exe)
            .arg(&home)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("cannot start updater")?;
    }
    tracing::info!(repo = %repo.display(), "self-update started; exiting so the binary can be replaced");
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(800)).await;
        std::process::exit(0);
    });
    Ok(log)
}

pub fn status() -> UpdateStatus {
    let home = config::triad_home();
    let last = std::fs::read_to_string(home.join("update.status")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let log = std::fs::read_to_string(home.join("update.log")).unwrap_or_default();
    let tail: Vec<&str> = log.lines().rev().take(40).collect();
    UpdateStatus { running: matches!(last.as_deref(), Some("running") | Some("starting")), last, log_tail: tail.into_iter().rev().collect::<Vec<_>>().join("\n") }
}

/// Forget the "ok …" note once it has been shown.
pub fn ack() {
    let p = config::triad_home().join("update.status");
    if let Ok(s) = std::fs::read_to_string(&p) {
        if s.starts_with("ok") || s.starts_with("fail") {
            let _ = std::fs::remove_file(&p);
        }
    }
}
