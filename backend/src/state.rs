//! Shared application state injected into every axum handler. `db`/`events`/`turn_lock`
//! belong to the *current room*; `for_room` re-scopes a clone to another room.

use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::{
    config::{Config, LlmSettings},
    models::RoomEvent,
    openrouter::LlmClient,
    rooms::RoomManager,
    update::UpdateInfo,
};

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub settings: Arc<RwLock<LlmSettings>>,
    pub llm: Arc<LlmClient>,
    pub update: Arc<RwLock<Option<UpdateInfo>>>,
    pub rooms: Arc<RoomManager>,
    pub bot_username: Arc<RwLock<Option<String>>>,
    // ── current room ──
    pub room_id: String,
    pub db: SqlitePool,
    pub events: broadcast::Sender<RoomEvent>,
    /// Rounds are serialized per room: a tick spam-click never interleaves with a chat round.
    pub turn_lock: Arc<Mutex<()>>,
}

impl AppState {
    pub fn emit(&self, ev: RoomEvent) {
        // No subscribers is not an error.
        let _ = self.events.send(ev);
    }

    pub async fn llm_settings(&self) -> LlmSettings {
        self.settings.read().await.clone()
    }

    pub async fn for_room(&self, room_id: &str) -> anyhow::Result<AppState> {
        if room_id == self.room_id {
            return Ok(self.clone());
        }
        let h = self.rooms.get(room_id).await?;
        Ok(AppState {
            room_id: room_id.to_string(),
            db: h.db,
            events: h.events,
            turn_lock: h.turn_lock,
            ..self.clone()
        })
    }
}
