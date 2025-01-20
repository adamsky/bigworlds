//! Worker protocol.

use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::server::ServerId;
use crate::worker::WorkerId;
use crate::{Address, EntityId, EventName, Model, Query, Result, Var};

use super::{leader, server};

/// Local protocol is not meant to be serialized for over-the-wire transfer.
#[derive(Clone)]
pub enum RequestLocal {
    /// Introduce worker to leader with local channel.
    ///
    /// # Auth
    ///
    /// No auth is performed as requesting this already requires access to
    /// the runtime and relevant channels.
    ConnectToLeader(
        LocalExec<(WorkerId, leader::RequestLocal), Result<leader::Response>>,
        LocalExec<RequestLocal, Result<Response>>,
    ),
    ConnectToServer(
        Option<ServerId>,
        LocalExec<(Option<WorkerId>, server::RequestLocal), Result<server::Response>>,
        LocalExec<RequestLocal, Result<Response>>,
    ),
    ConnectToWorker(),
    IntroduceLeader(LocalExec<(WorkerId, leader::RequestLocal), Result<leader::Response>>),
    ConnectAndRegisterServer(
        LocalExec<(Option<WorkerId>, server::RequestLocal), Result<server::Response>>,
    ),
    Request(Request),
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    ConnectToLeader(Option<CompositeAddress>),
    /// Instructs the worker to reach out to a remote worker.
    ConnectToWorker(CompositeAddress),
    /// Introduces the calling worker to the remote worker.
    IntroduceWorker(WorkerId),
    GetLeader,

    /// Gets the addresses at which the worker can be reached over the network.
    GetListeners,

    /// Gets worker status.
    Status,

    NewRequirements {
        ram_mb: usize,
        disk_mb: usize,
        transfer_mb: usize,
    },

    Ping(Vec<u8>),
    MemorySize,
    Entities,
    Clock,
    Initialize,
    Step,
    IsBlocking {
        wait: bool,
    },

    EntityList,

    Query(Query),

    /// Sets the interface receiver as "blocking" in terms of step advance
    SetBlocking(bool),

    /// Returns the current model
    GetModel,
    /// Pulling a model is initiated by server client
    PullModel(Model),
    /// Setting a model is more authoritative and comes from leader
    // TODO: provide rationale behind the distinction
    SetModel(Model),

    /// Retrieves data from a single variable
    GetVar(Address),
    /// Performs an overwrite of existing data at the given address
    SetVar(Address, Var),

    Trigger(EventName),

    SpawnEntity {
        name: String,
        prefab: String,
    },
}

impl Into<RequestLocal> for Request {
    fn into(self) -> RequestLocal {
        RequestLocal::Request(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    ConnectToLeader {
        worker_id: WorkerId,
    },
    ConnectToServer {
        worker_id: WorkerId,
    },
    IntroduceWorker(WorkerId),
    GetLeader(Option<std::net::SocketAddr>),

    GetListeners(Vec<CompositeAddress>),

    ClusterStatus {
        worker_count: usize,
    },
    NewRequirements,
    Empty,

    MemorySize(usize),
    Ping(Vec<u8>),
    GetModel(Model),
    Entities {
        machined: Vec<EntityId>,
        non_machined: Vec<EntityId>,
    },
    Clock(usize),
    Step,
    IsBlocking(bool),

    Register {
        server_id: ServerId,
    },
    EntityList(Vec<crate::EntityName>),
    Query(crate::QueryProduct),
    PullProject,
    StepUntil,

    Status {
        uptime: usize,
        worker_count: u32,
    },

    GetVar(Var),
}

use crate::Error;

impl TryInto<Model> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<Model, Self::Error> {
        match self {
            Response::GetModel(model) => Ok(model),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}

impl TryInto<Var> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<Var, Self::Error> {
        match self {
            Response::GetVar(var) => Ok(var),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}

impl TryInto<crate::QueryProduct> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<crate::QueryProduct, Self::Error> {
        match self {
            Response::Query(product) => Ok(product),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}
