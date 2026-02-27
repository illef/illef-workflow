use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::{Local, Utc};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

use crate::common::config::scripts_dir;
use crate::common::db::{insert_execution, logs_dir, update_execution_finished};
use crate::common::types::{Execution, ExecutionStatus, NotificationConfig};

pub fn log_path_for(workflow: &str, execution_id: &str) -> PathBuf {
    logs_dir()
        .join(workflow)
        .join(format!("{}.log", execution_id))
}

pub async fn execute_workflow(
    workflow_name: String,
    script: String,
    message_script: Option<String>,
    db: Arc<Mutex<rusqlite::Connection>>,
    notification: NotificationConfig,
) -> Result<()> {
    let execution_id = Uuid::new_v4().to_string();
    let log_path = log_path_for(&workflow_name, &execution_id);

    if let Some(parent) = log_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let execution = Execution {
        id: execution_id.clone(),
        workflow: workflow_name.clone(),
        status: ExecutionStatus::Running,
        started_at: Utc::now(),
        finished_at: None,
        exit_code: None,
        log_path: log_path.to_string_lossy().to_string(),
    };

    {
        let conn = db.lock().await;
        insert_execution(&conn, &execution)?;
    }

    info!(workflow = %workflow_name, id = %execution_id, "execution started");

    let script_path = scripts_dir().join(&script);
    let mut log_file = File::create(&log_path).await?;

    let header = format!(
        "[{}] Starting workflow: {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        workflow_name
    );
    log_file.write_all(header.as_bytes()).await?;

    let output = Command::new("bash")
        .arg(&script_path)
        .output()
        .await;

    let (status, exit_code) = match output {
        Ok(out) => {
            log_file.write_all(&out.stdout).await?;
            if !out.stderr.is_empty() {
                log_file.write_all(b"\n[stderr]\n").await?;
                log_file.write_all(&out.stderr).await?;
            }

            let code = out.status.code().unwrap_or(-1);
            let finished_line = format!(
                "\n[{}] Finished with exit code: {}\n",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                code
            );
            log_file.write_all(finished_line.as_bytes()).await?;

            if out.status.success() {
                (ExecutionStatus::Success, code)
            } else {
                (ExecutionStatus::Failed, code)
            }
        }
        Err(e) => {
            let err_msg = format!("\n[error] Failed to start process: {}\n", e);
            log_file.write_all(err_msg.as_bytes()).await?;
            error!(workflow = %workflow_name, error = %e, "failed to start process");
            (ExecutionStatus::Failed, -1)
        }
    };

    let finished_at = Utc::now();

    {
        let conn = db.lock().await;
        update_execution_finished(&conn, &execution_id, status.clone(), finished_at, exit_code)?;
    }

    info!(
        workflow = %workflow_name,
        id = %execution_id,
        status = %status.as_str(),
        exit_code = exit_code,
        "execution finished"
    );

    let message_result = if status == ExecutionStatus::Success {
        run_message_script(message_script.as_deref()).await
    } else {
        MessageScriptResult::NoScript
    };
    if message_result != MessageScriptResult::Suppressed {
        let body = message_result.body();
        send_notification(&notification, &workflow_name, &status, body.as_deref()).await;
    }

    Ok(())
}

/// exit 0  → notification with stdout as body
/// exit 3  → suppress notification (nothing to report)
/// others  → notification with default body
#[derive(PartialEq)]
enum MessageScriptResult {
    NoScript,
    Body(String),
    Empty,
    Suppressed,
}

impl MessageScriptResult {
    fn body(self) -> Option<String> {
        match self {
            MessageScriptResult::Body(s) => Some(s),
            _ => None,
        }
    }
}

async fn run_message_script(message_script: Option<&str>) -> MessageScriptResult {
    let Some(script_name) = message_script else {
        return MessageScriptResult::NoScript;
    };
    let script_path = scripts_dir().join(script_name);
    let Ok(out) = Command::new("bash").arg(&script_path).output().await else {
        return MessageScriptResult::Empty;
    };

    match out.status.code() {
        Some(3) => MessageScriptResult::Suppressed,
        Some(0) => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                MessageScriptResult::Empty
            } else {
                MessageScriptResult::Body(text)
            }
        }
        _ => MessageScriptResult::Empty,
    }
}

async fn send_notification(
    notification: &NotificationConfig,
    workflow_name: &str,
    status: &ExecutionStatus,
    custom_body: Option<&str>,
) {
    let (title, default_body) = match status {
        ExecutionStatus::Success => (
            format!("{} succeeded", workflow_name),
            "completed successfully".to_string(),
        ),
        ExecutionStatus::Failed => (
            format!("{} failed", workflow_name),
            "".to_string(),
        ),
        ExecutionStatus::Running => return,
    };

    let body = custom_body.unwrap_or(&default_body);

    let parts: Vec<&str> = notification.command.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    let _ = Command::new(parts[0])
        .args(&parts[1..])
        .arg(title)
        .arg(body)
        .spawn();
}
