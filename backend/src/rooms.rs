//! One SQLite room per Telegram user (`~/.triad/rooms/tg_<id>.db`); the desktop keeps the
//! historical `local` room at DATABASE_URL. Rooms are opened lazily and cached.

use std::{collections::HashMap, sync::Arc};

use anyhow::{bail, Result};
use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex};

use crate::{config::Config, db, models::RoomEvent};

pub const LOCAL_ROOM: &str = "local";

#[derive(Clone)]
pub struct RoomHandle {
    pub db: SqlitePool,
    pub events: broadcast::Sender<RoomEvent>,
    pub turn_lock: Arc<Mutex<()>>,
    /// Player lines that arrived during a round and still need a reaction.
    pub pending: Arc<std::sync::atomic::AtomicUsize>,
}

pub struct RoomManager {
    cfg: Arc<Config>,
    rooms: Mutex<HashMap<String, RoomHandle>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub created_at: String,
    #[serde(default)]
    pub open: bool,
}

fn index_path() -> std::path::PathBuf {
    crate::config::triad_home().join("rooms").join("index.json")
}

fn read_index() -> Vec<RoomInfo> {
    std::fs::read_to_string(index_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn write_index(list: &[RoomInfo]) -> Result<()> {
    let p = index_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(list)?)?;
    Ok(())
}

impl RoomManager {
    pub fn new(cfg: Arc<Config>) -> Self {
        Self { cfg, rooms: Mutex::new(HashMap::new()) }
    }

    /// Ids of rooms opened in this process (the heartbeat only breathes into rooms someone used).
    pub async fn open_ids(&self) -> Vec<String> {
        self.rooms.lock().await.keys().cloned().collect()
    }

    /// Local yards: the historical `local` room plus every `yard_*` the player created.
    pub async fn list_local(&self) -> Result<Vec<RoomInfo>> {
        let open = self.open_ids().await;
        let mut list = vec![RoomInfo { id: LOCAL_ROOM.into(), name: read_index().iter().find(|r| r.id == LOCAL_ROOM).map(|r| r.name.clone()).unwrap_or_else(|| "Двор".into()), created_at: String::new(), open: open.contains(&LOCAL_ROOM.to_string()) }];
        for r in read_index().into_iter().filter(|r| r.id != LOCAL_ROOM) {
            list.push(RoomInfo { open: open.contains(&r.id), ..r });
        }
        Ok(list)
    }

    pub async fn create_local(&self, name: &str) -> Result<RoomInfo> {
        let name = crate::mermaid::truncate_chars(name.trim(), 40);
        if name.chars().count() < 2 {
            bail!("название слишком короткое");
        }
        let mut idx = read_index();
        let mut id = format!("yard_{}", crate::mermaid::safe_id(&name).chars().take(24).collect::<String>().trim_end_matches('_'));
        if id == "yard_" {
            id = format!("yard_{}", chrono::Utc::now().timestamp());
        }
        if idx.iter().any(|r| r.id == id) {
            id = format!("{id}_{}", chrono::Utc::now().timestamp() % 10_000);
        }
        let info = RoomInfo { id, name, created_at: chrono::Utc::now().to_rfc3339(), open: false };
        idx.push(info.clone());
        write_index(&idx)?;
        Ok(info)
    }

    pub async fn rename_local(&self, id: &str, name: &str) -> Result<()> {
        let name = crate::mermaid::truncate_chars(name.trim(), 40);
        if name.chars().count() < 2 {
            bail!("название слишком короткое");
        }
        let mut idx = read_index();
        if let Some(r) = idx.iter_mut().find(|r| r.id == id) {
            r.name = name;
        } else if id == LOCAL_ROOM {
            idx.push(RoomInfo { id: LOCAL_ROOM.into(), name, created_at: String::new(), open: false });
        } else {
            bail!("нет такого двора");
        }
        write_index(&idx)
    }

    /// Delete a yard: the database file is kept beside as `.deleted-<stamp>` (nothing is lost).
    pub async fn delete_local(&self, id: &str) -> Result<()> {
        if id == LOCAL_ROOM {
            bail!("основной двор удалить нельзя — очисти его кнопкой «новая комната»");
        }
        let mut idx = read_index();
        let before = idx.len();
        idx.retain(|r| r.id != id);
        if idx.len() == before {
            bail!("нет такого двора");
        }
        write_index(&idx)?;
        if let Some(h) = self.rooms.lock().await.remove(id) {
            h.db.close().await;
        }
        let dir = crate::config::triad_home().join("rooms");
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        for suffix in ["", "-wal", "-shm"] {
            let p = dir.join(format!("{id}.db{suffix}"));
            if p.exists() {
                let _ = std::fs::rename(&p, dir.join(format!("{id}.db{suffix}.deleted-{stamp}")));
            }
        }
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<RoomHandle> {
        if id.is_empty() || id.len() > 40 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            bail!("invalid room id");
        }
        let mut rooms = self.rooms.lock().await;
        if let Some(h) = rooms.get(id) {
            return Ok(h.clone());
        }
        let url = if id == LOCAL_ROOM {
            self.cfg.database_url.clone()
        } else {
            let dir = crate::config::triad_home().join("rooms");
            std::fs::create_dir_all(&dir)?;
            format!("sqlite://{}?mode=rwc", dir.join(format!("{id}.db")).display())
        };
        if let Err(e) = crate::backup::apply_pending(id, &url) {
            tracing::warn!(room = %id, error = %e, "pending restore failed");
        }
        let pool = db::connect(&url).await?;
        db::migrate(&pool).await?;
        db::seed_if_empty(&pool).await?;
        let (events, _) = broadcast::channel(256);
        let handle = RoomHandle { db: pool, events, turn_lock: Arc::new(Mutex::new(())), pending: Arc::new(std::sync::atomic::AtomicUsize::new(0)) };
        rooms.insert(id.to_string(), handle.clone());
        tracing::info!(room = %id, "room opened");
        Ok(handle)
    }
}
