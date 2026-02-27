use std::path::PathBuf;

use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use super::types::AppConfig;

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("illef-workflow")
        .join("config.yaml")
}

pub fn scripts_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("illef-workflow")
        .join("scripts")
}

pub fn load_config() -> Result<AppConfig> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    let config: AppConfig =
        serde_yaml::from_str(&content).with_context(|| "failed to parse config.yaml")?;
    Ok(config)
}

pub fn watch_config(tx: mpsc::Sender<()>) -> Result<RecommendedWatcher> {
    let path = config_path();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) {
                let _ = tx.try_send(());
            }
        }
    })?;

    if let Some(parent) = path.parent() {
        watcher.watch(parent, RecursiveMode::NonRecursive)?;
    }

    Ok(watcher)
}
