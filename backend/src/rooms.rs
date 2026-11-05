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
}

pub struct RoomManager {
    cfg: Arc<Config>,
    rooms: Mutex<HashMap<String, RoomHandle>>,
}

impl RoomManager {
    pub fn new(cfg: Arc<Config>) -> Self {
        Self { cfg, rooms: Mutex::new(HashMap::new()) }
    }

    /// Ids of rooms opened in this process (the heartbeat only breathes into rooms someone used).
    pub async fn open_ids(&self) -> Vec<String> {
        self.rooms.lock().await.keys().cloned().collect()
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
        let pool = db::connect(&url).await?;
        db::migrate(&pool).await?;
        db::seed_if_empty(&pool).await?;
        let (events, _) = broadcast::channel(256);
        let handle = RoomHandle { db: pool, events, turn_lock: Arc::new(Mutex::new(())) };
        rooms.insert(id.to_string(), handle.clone());
        tracing::info!(room = %id, "room opened");
        Ok(handle)
    }
}
