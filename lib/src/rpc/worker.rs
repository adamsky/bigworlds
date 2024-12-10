//! Worker protocol.
//!
//! # Cluster initialization
//!
//! On cluster initialization, leader connects to listed worker
//! addresses and sends introductory messages. Each worker creates
//! a list of all the other workers in the cluster. This way all
//! the workers can exchange information with each other without
//! the need for centralized broker. Each worker keeps a map of entities
//! and their current node location.
//!
//! Simulation initialization is signalled to workers by the leader.
//! Necessary data (sim model) is sent over the network to each of the
//! workers.
//!
//! ## Non-machine processing
//!
//! Processing a step requires handling incoming client chatter, which is
//! mostly event invokes and step process requests (client blocking mechanism).
//!
//! ## Machine processing
//!
//! Runtime-level machine step processing consists of two phases: local and
//! external.
//!
//! Local phase is performed in isolation by each of the workers.
//! During this phase any external commands that were invoked are collected
//! and stored.
//!
//! During the external phase each worker sends messages to other workers based
//! on what has been collected in the previous phase. Messages are sent to
//! proper peer nodes, since each external command is addressed to a specific
//! entity, and worker keeps a map of entities and nodes owning them. It also
//! has a map of nodes with I/O sockets, 2 sockets for each node.

use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::server::ServerId;
use crate::worker::WorkerId;
use crate::{EntityId, EventName, Model, Query, Result};

use super::{leader, server};

#[derive(Clone)]
pub enum RequestLocal {
    /// Introduce worker to leader with local channel.
    ///
    /// # Auth
    ///
    /// No auth is performed as requesting this already requires access to
    /// the runtime and relevant channels.
    ConnectToLeader(
        LocalExec<(Option<WorkerId>, leader::RequestLocal), Result<leader::Response>>,
        LocalExec<RequestLocal, Result<Response>>,
    ),
    ConnectToServer(
        Option<ServerId>,
        LocalExec<(Option<WorkerId>, server::RequestLocal), Result<server::Response>>,
        LocalExec<(Option<ServerId>, RequestLocal), Result<Response>>,
    ),
    ConnectLeader(LocalExec<(Option<WorkerId>, leader::RequestLocal), Result<leader::Response>>),
    ConnectAndRegisterServer(
        LocalExec<(Option<WorkerId>, server::RequestLocal), Result<server::Response>>,
    ),
    Request(Request),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    WorkerStatus,
    ConnectToLeader {
        address: CompositeAddress,
    },
    NewRequirements {
        ram_mb: usize,
        disk_mb: usize,
        transfer_mb: usize,
    },

    Ping(Vec<u8>),
    MemorySize,
    Model,
    Entities,
    Clock,
    Initialize {
        model: Model,
        mod_script: bool,
    },
    Step,
    IsBlocking {
        wait: bool,
    },

    EntityList,

    Query(Query),

    /// Sets the interface receiver as "blocking" in terms of step advance
    SetBlocking(bool),

    PullModel(Model),

    Status,

    Trigger(EventName),
}

impl Into<RequestLocal> for Request {
    fn into(self) -> RequestLocal {
        RequestLocal::Request(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    WorkerStatus {
        uptime: usize,
    },
    ClusterStatus {
        worker_count: usize,
    },
    NewRequirements,
    Empty,

    Connect,
    MemorySize(usize),
    Ping(Vec<u8>),
    Model(Model),
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
        worker_count: u32,
    },
}

impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            _ => write!(f, "response doesn't implement display yet"),
        }
    }
}
