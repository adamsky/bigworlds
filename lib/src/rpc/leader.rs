use serde::{Deserialize, Serialize};

use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::worker::WorkerId;
use crate::{Model, Result};

use super::worker;

pub type Token = uuid::Uuid;

#[derive(Clone)]
pub enum RequestLocal {
    /// Introduce worker to leader with local channel.
    ///
    /// # Auth
    ///
    /// No auth is performed as requesting this already requires access to
    /// the runtime and relevant channels.
    ConnectToWorker(
        LocalExec<worker::RequestLocal, Result<worker::Response>>,
        LocalExec<(Option<WorkerId>, RequestLocal), Result<Response>>,
    ),
    ConnectAndRegisterWorker(LocalExec<worker::RequestLocal, Result<worker::Response>>),
    Request(Request),
}

impl From<Request> for RequestLocal {
    fn from(value: Request) -> Self {
        Self::Request(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    Status,
    ConnectToWorker {
        address: CompositeAddress,
    },
    /// Request leader to pull incoming model as current model
    PullModel(Model),
    Initialize {
        scenario: Option<String>,
    },
    Step,

    /// Request simple echoing of sent bytes
    Ping(Vec<u8>),
    /// Introduce worker to leader
    ///
    /// # Authorization
    ///
    ///
    Register(Token),
    /// Request the current simulation clock value on the leader
    Clock,
    /// Request the complete model from leader
    Model,

    ReadyUntil(usize),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    Empty,

    WorkerStatus { uptime: usize },
    ClusterStatus { worker_count: usize },

    Ping(Vec<u8>),
    Connect,
    Register { worker_id: WorkerId },
    Clock(usize),
    Model(Model),
    PullModel,
    StepUntil,
}
