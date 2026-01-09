use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub struct AppState {
    // Only need Sender to signal stop. Receivers are created by subscribe().
    pub server_stop_tx: Arc<Mutex<Option<broadcast::Sender<()>>>>, 
    pub video_tx: broadcast::Sender<Vec<u8>>,
    pub header: Arc<Mutex<Option<Vec<u8>>>>,
}

impl Default for AppState {
    fn default() -> Self {
        // Increase buffer size to handle more chunks before lagging
        let (tx, _rx) = broadcast::channel(1000);
        Self {
            server_stop_tx: Arc::new(Mutex::new(None)),
            video_tx: tx,
            header: Arc::new(Mutex::new(None)),
        }
    }
}
