use anyhow::Result;
use hyper_util::rt::TokioIo;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use crate::proto::workflow_service_client::WorkflowServiceClient;
use crate::proto::{Empty, ExecutionRequest, WorkflowRequest};
use crate::runner::server::SOCKET_PATH;

pub async fn connect() -> Result<WorkflowServiceClient<Channel>> {
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(|_: Uri| async {
            let stream = tokio::net::UnixStream::connect(SOCKET_PATH).await?;
            Ok::<_, std::io::Error>(TokioIo::new(stream))
        }))
        .await?;

    Ok(WorkflowServiceClient::new(channel))
}

pub async fn list_workflows(
    client: &mut WorkflowServiceClient<Channel>,
) -> Result<Vec<crate::proto::WorkflowInfo>> {
    let response = client.list_workflows(Empty {}).await?;
    Ok(response.into_inner().workflows)
}

pub async fn get_workflow_status(
    client: &mut WorkflowServiceClient<Channel>,
    name: &str,
) -> Result<crate::proto::WorkflowStatusResponse> {
    let response = client
        .get_workflow_status(WorkflowRequest {
            name: name.to_string(),
        })
        .await?;
    Ok(response.into_inner())
}

pub async fn get_execution_log_path(
    client: &mut WorkflowServiceClient<Channel>,
    execution_id: &str,
) -> Result<String> {
    let response = client
        .get_execution_log_path(ExecutionRequest {
            execution_id: execution_id.to_string(),
        })
        .await?;
    Ok(response.into_inner().log_path)
}

pub async fn trigger_workflow(
    client: &mut WorkflowServiceClient<Channel>,
    name: &str,
) -> Result<crate::proto::TriggerResponse> {
    let response = client
        .trigger_workflow(WorkflowRequest {
            name: name.to_string(),
        })
        .await?;
    Ok(response.into_inner())
}
