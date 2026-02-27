pub mod executor;
pub mod scheduler;
pub mod server;

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{Mutex, mpsc};
use tracing::info;

use crate::common::config::{load_config, watch_config};
use crate::common::db::open_db;
use crate::runner::scheduler::SchedulerCommand;

pub async fn run() -> Result<()> {
    let config = load_config()?;
    info!(workflows = config.workflows.len(), "config loaded");

    let db = Arc::new(Mutex::new(open_db()?));

    let (scheduler_tx, _scheduler_handle) = scheduler::start(config.clone(), Arc::clone(&db));

    // config hot-reload
    let (config_tx, mut config_rx) = mpsc::channel::<()>(4);
    let _watcher = watch_config(config_tx)?;
    let scheduler_tx_clone = scheduler_tx.clone();
    tokio::spawn(async move {
        while config_rx.recv().await.is_some() {
            match load_config() {
                Ok(new_config) => {
                    info!("config changed, reloading scheduler");
                    let _ = scheduler_tx_clone
                        .send(SchedulerCommand::Reload(new_config))
                        .await;
                }
                Err(e) => {
                    tracing::error!("config reload failed: {}", e);
                }
            }
        }
    });

    // run gRPC server (blocking)
    server::serve(Arc::clone(&db), scheduler_tx).await?;

    Ok(())
}
