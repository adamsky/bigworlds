use fnv::FnvHashMap;
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
        LocalExec<(WorkerId, RequestLocal), Result<Response>>,
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
    /// Instruct the leader to reach out to the worker at provided address
    ConnectToWorker(CompositeAddress),
    /// Introduce the calling worker to the leader
    // TODO: auth
    IntroduceWorker(WorkerId),

    /// Request full status update of the leader
    Status,
    /// Request leader to pull incoming model as current model
    // TODO: perhaps allow more granular *changes* to the model?
    PullModel(Model),
    /// Make leader initialize the cluster, optionally using the provided
    /// scenario
    Initialize {
        scenario: Option<String>,
    },
    /// Step through the simulation
    Step,

    /// Request the current simulation clock value on the leader
    Clock,
    /// Request a list of all currently connected workers
    GetWorkers,
    /// Request the complete model from leader
    Model,

    ReadyUntil(usize),

    /// Spawn entity based on the provided name prefab
    SpawnEntity {
        name: String,
        prefab: String,
    },

    /// Request simple echoing of sent bytes
    Ping(Vec<u8>),
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display)]
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
    GetWorkers(FnvHashMap<WorkerId, Vec<CompositeAddress>>),
    Model(Model),
    PullModel,
    StepUntil,
}
