pub mod common;
pub mod runner;
pub mod tui;

pub mod proto {
    tonic::include_proto!("workflow");
}
