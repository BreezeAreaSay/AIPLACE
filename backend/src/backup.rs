//! Room backup: one zip with a consistent SQLite snapshot (`VACUUM INTO`), every attachment and
//! a manifest. Restore unpacks into `$TRIAD_HOME/restore_pending/<room>/` and the room manager
//! swaps it in on the next start (the live pool must be closed first).

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::{config, files, state::AppState};

fn db_file_of(url: &str) -> Option<PathBuf> {
    let p = url.strip_prefix("sqlite://").or_else(|| url.strip_prefix("sqlite:"))?;
    let p = p.split('?').next()?;
    if p.is_empty() || p == ":memory:" {
        return None;
    }
    Some(PathBuf::from(p))
}

pub fn pending_dir(room_id: &str) -> PathBuf {
    config::triad_home().join("restore_pending").join(crate::mermaid::safe_id(room_id))
}

/// Build the zip in memory (rooms are small; attachments are capped at 50 MB each).
pub async fn make_zip(app: &AppState) -> Result<Vec<u8>> {
    let tmp = std::env::temp_dir().join(format!("triad-backup-{}-{}.db", crate::mermaid::safe_id(&app.room_id), chrono::Utc::now().timestamp()));
    let _ = std::fs::remove_file(&tmp);
    sqlx::query(&format!("VACUUM INTO '{}'", tmp.display().to_string().replace('\'', "''"))).execute(&app.db).await.context("VACUUM INTO")?;
    let db_bytes = std::fs::read(&tmp).context("read snapshot")?;
    let _ = std::fs::remove_file(&tmp);

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("room.db", opts)?;
        zip.write_all(&db_bytes)?;
        let mut files_n = 0usize;
        let dir = files::room_dir(&app.room_id);
        if dir.is_dir() {
            for entry in std::fs::read_dir(&dir)?.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let name = p.file_name().unwrap().to_string_lossy().to_string();
                    zip.start_file(format!("files/{name}"), opts)?;
                    zip.write_all(&std::fs::read(&p)?)?;
                    files_n += 1;
                }
            }
        }
        let manifest = serde_json::json!({
            "app": "triad-canvas",
            "version": crate::update::VERSION,
            "room_id": app.room_id,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "files": files_n,
            "db_bytes": db_bytes.len(),
        });
        zip.start_file("manifest.json", opts)?;
        zip.write_all(manifest.to_string().as_bytes())?;
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

/// Validate and stage a backup for the next start. Returns the manifest.
pub fn stage_restore(room_id: &str, bytes: &[u8]) -> Result<serde_json::Value> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("это не zip")?;
    let dir = pending_dir(room_id);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("files"))?;
    let mut manifest = serde_json::json!({});
    let mut has_db = false;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        if name == "room.db" {
            anyhow::ensure!(data.starts_with(b"SQLite format 3"), "room.db не похож на базу SQLite");
            std::fs::write(dir.join("room.db"), &data)?;
            has_db = true;
        } else if name == "manifest.json" {
            manifest = serde_json::from_slice(&data).unwrap_or_default();
        } else if let Some(rest) = name.strip_prefix("files/") {
            let safe: String = rest.chars().filter(|c| !matches!(c, '/' | '\\')).collect();
            if !safe.is_empty() {
                std::fs::write(dir.join("files").join(safe), &data)?;
            }
        }
    }
    anyhow::ensure!(has_db, "в архиве нет room.db");
    std::fs::write(dir.join("READY"), chrono::Utc::now().to_rfc3339())?;
    Ok(manifest)
}

/// Called by the room manager before opening a pool: if a staged restore exists, swap it in.
pub fn apply_pending(room_id: &str, database_url: &str) -> Result<bool> {
    let dir = pending_dir(room_id);
    if !dir.join("READY").exists() {
        return Ok(false);
    }
    let Some(db_path) = db_file_of(database_url) else { anyhow::bail!("restore needs a file-backed database") };
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    // keep the previous room next to it, just in case
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    if db_path.exists() {
        let bak = db_path.with_extension(format!("db.before-restore-{stamp}"));
        std::fs::rename(&db_path, &bak).ok();
    }
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(PathBuf::from(format!("{}{suffix}", db_path.display())));
    }
    std::fs::copy(dir.join("room.db"), &db_path).context("copy restored db")?;
    let files_dir = files::room_dir(room_id);
    if files_dir.exists() {
        let bak = files_dir.with_file_name(format!("{}.before-restore-{stamp}", files_dir.file_name().unwrap().to_string_lossy()));
        std::fs::rename(&files_dir, &bak).ok();
    }
    std::fs::create_dir_all(&files_dir)?;
    for entry in std::fs::read_dir(dir.join("files"))?.flatten() {
        let p = entry.path();
        if p.is_file() {
            std::fs::copy(&p, files_dir.join(p.file_name().unwrap()))?;
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    tracing::info!(room = %room_id, "restored room from backup");
    Ok(true)
}

pub fn pending_info(room_id: &str) -> Option<String> {
    std::fs::read_to_string(pending_dir(room_id).join("READY")).ok()
}

pub fn cancel_pending(room_id: &str) {
    let _ = std::fs::remove_dir_all(pending_dir(room_id));
}

#[allow(dead_code)]
fn _p(_: &Path) {}

// ───────────────────────────── Snapshots (server-side) ─────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotInfo {
    pub name: String,
    pub bytes: u64,
    pub created_at: String,
    pub kind: String,
}

pub fn snapshots_dir(room_id: &str) -> PathBuf {
    config::triad_home().join("backups").join(crate::mermaid::safe_id(room_id))
}

/// Write a zip into `~/.triad/backups/<room>/`, keep the newest 14 automatic ones.
pub async fn snapshot(app: &AppState, kind: &str) -> Result<SnapshotInfo> {
    let bytes = make_zip(app).await?;
    let dir = snapshots_dir(&app.room_id);
    std::fs::create_dir_all(&dir)?;
    let name = format!("{}-{}.zip", chrono::Local::now().format("%Y%m%d-%H%M%S"), kind);
    std::fs::write(dir.join(&name), &bytes)?;
    if kind == "auto" {
        let mut autos: Vec<_> = list_snapshots(&app.room_id)?.into_iter().filter(|s| s.kind == "auto").collect();
        autos.sort_by(|a, b| b.name.cmp(&a.name));
        for old in autos.into_iter().skip(14) {
            let _ = std::fs::remove_file(dir.join(&old.name));
        }
    }
    Ok(SnapshotInfo { name, bytes: bytes.len() as u64, created_at: chrono::Utc::now().to_rfc3339(), kind: kind.into() })
}

pub fn list_snapshots(room_id: &str) -> Result<Vec<SnapshotInfo>> {
    let dir = snapshots_dir(room_id);
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".zip") {
                continue;
            }
            let meta = e.metadata()?;
            let created = meta.modified().ok().map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()).unwrap_or_default();
            let kind = name.trim_end_matches(".zip").rsplit('-').next().unwrap_or("manual").to_string();
            out.push(SnapshotInfo { name, bytes: meta.len(), created_at: created, kind });
        }
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

fn safe_snapshot_name(name: &str) -> Result<String> {
    anyhow::ensure!(name.ends_with(".zip") && !name.contains('/') && !name.contains('\\') && !name.contains(".."), "плохое имя снимка");
    Ok(name.to_string())
}

pub fn read_snapshot(room_id: &str, name: &str) -> Result<Vec<u8>> {
    let name = safe_snapshot_name(name)?;
    Ok(std::fs::read(snapshots_dir(room_id).join(name))?)
}

pub fn delete_snapshot(room_id: &str, name: &str) -> Result<()> {
    let name = safe_snapshot_name(name)?;
    std::fs::remove_file(snapshots_dir(room_id).join(name))?;
    Ok(())
}
