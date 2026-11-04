//! Shared application state injected into every axum handler.

use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex};

use crate::{config::Config, models::RoomEvent, openrouter::LlmClient};

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub db: SqlitePool,
    pub llm: Arc<LlmClient>,
    pub events: broadcast::Sender<RoomEvent>,
    /// Rounds are serialized: a tick spam-click never interleaves with a chat round.
    pub turn_lock: Arc<Mutex<()>>,
}

impl AppState {
    pub fn emit(&self, ev: RoomEvent) {
        // No subscribers is not an error.
        let _ = self.events.send(ev);
    }
}
