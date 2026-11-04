//! Shared application state injected into every axum handler.

use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::{
    config::{Config, LlmSettings},
    models::RoomEvent,
    openrouter::LlmClient,
    update::UpdateInfo,
};

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub settings: Arc<RwLock<LlmSettings>>,
    pub db: SqlitePool,
    pub llm: Arc<LlmClient>,
    pub events: broadcast::Sender<RoomEvent>,
    /// Rounds are serialized: a tick spam-click never interleaves with a chat round.
    pub turn_lock: Arc<Mutex<()>>,
    pub update: Arc<RwLock<Option<UpdateInfo>>>,
}

impl AppState {
    pub fn emit(&self, ev: RoomEvent) {
        // No subscribers is not an error.
        let _ = self.events.send(ev);
    }

    pub async fn llm_settings(&self) -> LlmSettings {
        self.settings.read().await.clone()
    }
}
