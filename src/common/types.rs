use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub name: String,
    pub cron: String,
    pub script: String,
    /// Optional script whose stdout becomes the notify-send body on success.
    #[serde(default)]
    pub message_script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub workflows: Vec<WorkflowConfig>,
    #[serde(default)]
    pub notifications: NotificationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub command: String,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            command: "notify-send".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    Running,
    Success,
    Failed,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Running => "running",
            ExecutionStatus::Success => "success",
            ExecutionStatus::Failed => "failed",
        }
    }
}

impl std::str::FromStr for ExecutionStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(ExecutionStatus::Running),
            "success" => Ok(ExecutionStatus::Success),
            "failed" => Ok(ExecutionStatus::Failed),
            _ => Err(anyhow::anyhow!("unknown status: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Execution {
    pub id: String,
    pub workflow: String,
    pub status: ExecutionStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub log_path: String,
}
