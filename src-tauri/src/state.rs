//! Shared application state.

use crate::aria2::Aria2;
use crate::config::Config;
use crate::hub::HubClient;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub app_data: PathBuf,
    pub config: std::sync::RwLock<Config>,
    /// Rebuilt when the proxy configuration changes.
    pub hub: std::sync::RwLock<HubClient>,
    pub aria2: tokio::sync::Mutex<Option<Aria2>>,
    pub tasks: std::sync::RwLock<Vec<crate::downloads::DownloadTask>>,
    pub sys: Mutex<sysinfo::System>,
    pub disks: Mutex<sysinfo::Disks>,
    pub gpu: Mutex<Option<crate::stats::GpuInfo>>,
}

impl AppState {
    pub fn config_clone(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    /// Cheap clone of the current HTTP client.
    pub fn hub(&self) -> HubClient {
        self.hub.read().unwrap().clone()
    }

    pub fn save_config(&self, cfg: &Config) -> Result<(), String> {
        crate::config::save(&self.app_data, cfg)?;
        *self.config.write().unwrap() = cfg.clone();
        Ok(())
    }
}

pub fn new_task_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| format!("{:x}", rng.gen_range(0..16)))
        .collect()
}
