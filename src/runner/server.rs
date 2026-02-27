use std::sync::Arc;

use anyhow::Result;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::{Request, Response, Status, transport::Server};
use tracing::info;

use crate::common::config::load_config;
use crate::common::db::{get_execution_by_id, get_executions, get_last_execution};
use crate::common::types::ExecutionStatus;
use crate::proto::workflow_service_server::{WorkflowService, WorkflowServiceServer};
use crate::proto::{
    Empty, ExecutionInfo, ExecutionRequest, ListWorkflowsResponse, LogPathResponse,
    TriggerResponse, WorkflowInfo, WorkflowRequest, WorkflowStatusResponse,
};
use crate::runner::scheduler::{SchedulerCommand, get_next_run};

pub const SOCKET_PATH: &str = "/tmp/illef-workflow.sock";

pub struct WorkflowServiceImpl {
    db: Arc<Mutex<rusqlite::Connection>>,
    scheduler_tx: tokio::sync::mpsc::Sender<SchedulerCommand>,
}

impl WorkflowServiceImpl {
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        scheduler_tx: tokio::sync::mpsc::Sender<SchedulerCommand>,
    ) -> Self {
        Self { db, scheduler_tx }
    }
}

fn execution_to_proto(exec: &crate::common::types::Execution) -> ExecutionInfo {
    ExecutionInfo {
        id: exec.id.clone(),
        workflow: exec.workflow.clone(),
        status: exec.status.as_str().to_string(),
        started_at: exec.started_at.timestamp(),
        finished_at: exec.finished_at.map(|t| t.timestamp()).unwrap_or(0),
        exit_code: exec.exit_code.unwrap_or(-1),
        log_path: exec.log_path.clone(),
    }
}

#[tonic::async_trait]
impl WorkflowService for WorkflowServiceImpl {
    async fn list_workflows(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListWorkflowsResponse>, Status> {
        let config = load_config().map_err(|e| Status::internal(e.to_string()))?;
        let conn = self.db.lock().await;

        let mut workflows = Vec::new();
        for wf in &config.workflows {
            let last = get_last_execution(&conn, &wf.name)
                .unwrap_or(None);
            let next_run_at = get_next_run(&wf.cron)
                .map(|t| t.timestamp())
                .unwrap_or(0);

            let (last_run_at, last_run_status) = match &last {
                Some(exec) => (
                    exec.started_at.timestamp(),
                    exec.status.as_str().to_string(),
                ),
                None => (0, String::new()),
            };

            let status = match &last {
                Some(exec) if exec.status == ExecutionStatus::Running => "running",
                _ => "idle",
            };

            workflows.push(WorkflowInfo {
                name: wf.name.clone(),
                cron: wf.cron.clone(),
                script: wf.script.clone(),
                status: status.to_string(),
                next_run_at,
                last_run_at,
                last_run_status,
            });
        }

        Ok(Response::new(ListWorkflowsResponse { workflows }))
    }

    async fn get_workflow_status(
        &self,
        request: Request<WorkflowRequest>,
    ) -> Result<Response<WorkflowStatusResponse>, Status> {
        let name = request.into_inner().name;
        let config = load_config().map_err(|e| Status::internal(e.to_string()))?;

        let wf_config = config
            .workflows
            .iter()
            .find(|w| w.name == name)
            .ok_or_else(|| Status::not_found(format!("workflow not found: {}", name)))?;

        let conn = self.db.lock().await;
        let executions = get_executions(&conn, &name, 50)
            .map_err(|e| Status::internal(e.to_string()))?;

        let last = executions.first();
        let next_run_at = get_next_run(&wf_config.cron)
            .map(|t| t.timestamp())
            .unwrap_or(0);

        let (last_run_at, last_run_status) = match last {
            Some(exec) => (exec.started_at.timestamp(), exec.status.as_str().to_string()),
            None => (0, String::new()),
        };

        let status = match last {
            Some(exec) if exec.status == ExecutionStatus::Running => "running",
            _ => "idle",
        };

        let workflow_info = WorkflowInfo {
            name: wf_config.name.clone(),
            cron: wf_config.cron.clone(),
            script: wf_config.script.clone(),
            status: status.to_string(),
            next_run_at,
            last_run_at,
            last_run_status,
        };

        Ok(Response::new(WorkflowStatusResponse {
            workflow: Some(workflow_info),
            executions: executions.iter().map(execution_to_proto).collect(),
        }))
    }

    async fn get_execution_log_path(
        &self,
        request: Request<ExecutionRequest>,
    ) -> Result<Response<LogPathResponse>, Status> {
        let execution_id = request.into_inner().execution_id;
        let conn = self.db.lock().await;

        let exec = get_execution_by_id(&conn, &execution_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("execution not found: {}", execution_id)))?;

        Ok(Response::new(LogPathResponse {
            log_path: exec.log_path,
        }))
    }

    async fn trigger_workflow(
        &self,
        request: Request<WorkflowRequest>,
    ) -> Result<Response<TriggerResponse>, Status> {
        let name = request.into_inner().name;

        let config = load_config().map_err(|e| Status::internal(e.to_string()))?;
        if !config.workflows.iter().any(|w| w.name == name) {
            return Err(Status::not_found(format!("workflow not found: {}", name)));
        }

        self.scheduler_tx
            .send(SchedulerCommand::Trigger(name.clone()))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(TriggerResponse {
            queued: false,
            message: format!("workflow {} triggered", name),
        }))
    }
}

pub async fn serve(
    db: Arc<Mutex<rusqlite::Connection>>,
    scheduler_tx: tokio::sync::mpsc::Sender<SchedulerCommand>,
) -> Result<()> {
    let socket_path = std::path::Path::new(SOCKET_PATH);
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(SOCKET_PATH)?;
    info!("gRPC server listening on {}", SOCKET_PATH);

    let service = WorkflowServiceImpl::new(db, scheduler_tx);

    Server::builder()
        .add_service(WorkflowServiceServer::new(service))
        .serve_with_incoming(UnixListenerStream::new(listener))
        .await?;

    Ok(())
}
