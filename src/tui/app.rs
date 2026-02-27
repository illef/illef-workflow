use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;

use anyhow::Result;
use tonic::transport::Channel;

use crate::proto::{ExecutionInfo, WorkflowInfo};
use crate::proto::workflow_service_client::WorkflowServiceClient;
use crate::tui::client;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Workflows,
    Executions,
    Log,
}

pub struct App {
    pub client: WorkflowServiceClient<Channel>,

    pub workflows: Vec<WorkflowInfo>,
    pub selected_workflow: usize,

    pub executions: Vec<ExecutionInfo>,
    pub selected_execution: usize,

    pub log_lines: Vec<String>,
    pub log_scroll: usize,
    pub log_path: Option<PathBuf>,
    pub log_file_pos: u64,

    pub active_panel: Panel,
    pub status_message: String,
    pub should_quit: bool,
}

impl App {
    pub async fn new() -> Result<Self> {
        let client = client::connect().await?;
        Ok(Self {
            client,
            workflows: Vec::new(),
            selected_workflow: 0,
            executions: Vec::new(),
            selected_execution: 0,
            log_lines: Vec::new(),
            log_scroll: 0,
            log_path: None,
            log_file_pos: 0,
            active_panel: Panel::Workflows,
            status_message: String::new(),
            should_quit: false,
        })
    }

    pub async fn refresh_workflows(&mut self) -> Result<()> {
        self.workflows = client::list_workflows(&mut self.client).await?;
        if self.selected_workflow >= self.workflows.len() && !self.workflows.is_empty() {
            self.selected_workflow = self.workflows.len() - 1;
        }
        Ok(())
    }

    pub async fn refresh_executions(&mut self) -> Result<()> {
        if let Some(wf) = self.workflows.get(self.selected_workflow) {
            let name = wf.name.clone();
            let status = client::get_workflow_status(&mut self.client, &name).await?;
            self.executions = status.executions;
            if self.selected_execution >= self.executions.len() && !self.executions.is_empty() {
                self.selected_execution = self.executions.len() - 1;
            }
        }
        Ok(())
    }

    pub fn select_workflow(&mut self, idx: usize) {
        self.selected_workflow = idx;
        self.executions.clear();
        self.log_lines.clear();
        self.log_path = None;
        self.log_file_pos = 0;
        self.selected_execution = 0;
    }

    pub fn select_execution(&mut self, idx: usize) {
        self.selected_execution = idx;
        self.log_lines.clear();
        self.log_scroll = 0;
        self.log_file_pos = 0;

        if let Some(exec) = self.executions.get(idx) {
            let path = PathBuf::from(&exec.log_path);
            if path.exists() {
                self.log_path = Some(path);
                self.load_log_from_start();
            } else {
                self.log_path = None;
                self.status_message = format!("Log file not found: {}", exec.log_path);
            }
        }
    }

    fn load_log_from_start(&mut self) {
        let Some(path) = &self.log_path else { return };
        let Ok(file) = File::open(path) else { return };
        let reader = BufReader::new(file);
        self.log_lines = reader.lines().map_while(Result::ok).collect();
        self.log_file_pos = std::fs::metadata(path)
            .map(|m| m.len())
            .unwrap_or(0);
        // scroll to bottom
        self.log_scroll = self.log_lines.len().saturating_sub(1);
    }

    pub fn poll_log_updates(&mut self) {
        let Some(path) = &self.log_path.clone() else { return };
        let Ok(metadata) = std::fs::metadata(path) else { return };
        let current_len = metadata.len();
        if current_len <= self.log_file_pos {
            return;
        }

        let Ok(mut file) = File::open(path) else { return };
        if file.seek(SeekFrom::Start(self.log_file_pos)).is_err() {
            return;
        }

        let reader = BufReader::new(&file);
        let new_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
        self.log_file_pos = current_len;

        let was_at_bottom = self.is_at_bottom();
        self.log_lines.extend(new_lines);
        if was_at_bottom {
            self.log_scroll = self.log_lines.len().saturating_sub(1);
        }
    }

    fn is_at_bottom(&self) -> bool {
        self.log_lines.is_empty() || self.log_scroll >= self.log_lines.len().saturating_sub(1)
    }

    pub fn scroll_log_up(&mut self) {
        self.log_scroll = self.log_scroll.saturating_sub(1);
    }

    pub fn scroll_log_down(&mut self) {
        if !self.log_lines.is_empty() {
            self.log_scroll = (self.log_scroll + 1).min(self.log_lines.len().saturating_sub(1));
        }
    }

    pub fn move_workflow_up(&mut self) {
        if self.selected_workflow > 0 {
            self.select_workflow(self.selected_workflow - 1);
        }
    }

    pub fn move_workflow_down(&mut self) {
        if self.selected_workflow + 1 < self.workflows.len() {
            self.select_workflow(self.selected_workflow + 1);
        }
    }

    pub fn move_execution_up(&mut self) {
        if self.selected_execution > 0 {
            self.select_execution(self.selected_execution - 1);
        }
    }

    pub fn move_execution_down(&mut self) {
        if self.selected_execution + 1 < self.executions.len() {
            self.select_execution(self.selected_execution + 1);
        }
    }

    pub async fn trigger_selected_workflow(&mut self) -> Result<()> {
        if let Some(wf) = self.workflows.get(self.selected_workflow) {
            let name = wf.name.clone();
            let resp = client::trigger_workflow(&mut self.client, &name).await?;
            self.status_message = resp.message;
        }
        Ok(())
    }

    pub fn selected_workflow_name(&self) -> Option<&str> {
        self.workflows.get(self.selected_workflow).map(|w| w.name.as_str())
    }
}
