use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::{leader, rpc, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Status,
    SpawnWorker {
        ram_mb: usize,
        disk_mb: usize,
        server_addr: Option<SocketAddr>,
        initial_requests: Vec<rpc::worker::Request>,
    },
    SpawnLeader {
        listeners: Vec<CompositeAddress>,
        config: leader::Config,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Status {
        worker_count: usize,
    },
    SpawnWorker {
        listeners: Vec<CompositeAddress>,
        server_addr: Option<CompositeAddress>,
    },
    SpawnLeader {
        listeners: Vec<CompositeAddress>,
    },
}
