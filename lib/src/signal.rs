use enum_as_inner::EnumAsInner;
use fnv::FnvHashMap;

#[cfg(feature = "machine")]
use crate::machine::cmd::CentralRemoteCommand;
#[cfg(feature = "machine")]
use crate::machine::ExecutionContext;

use crate::{
    Address, CompName, EntityId, EntityName, Model, PrefabName, Query, QueryProduct, StringId, Var,
    VarName,
};

pub type NodeId = u16;

/// Definition encompassing all possible messages available for communication
/// between two nodes and between node and central.
#[derive(Debug, Clone, Serialize, Deserialize, EnumAsInner)]
pub enum Signal {
    /// Request node to start initialization using given model and list of
    /// entities
    InitializeNode(Model),
    /// Request node to spawn a set of entities.
    SpawnEntities(Vec<(EntityId, Option<PrefabName>, Option<EntityName>)>),
    /// Request node to start processing step, includes event_queue vec
    StartProcessStep(Vec<StringId>),

    SnapshotRequest,

    WorkerConnected,

    WorkerStepAdvanceRequest(u32),
    WorkerStepAdvanceResponse,

    WorkerReady,
    WorkerNotReady,

    /// Shutdown imminent
    ShuttingDown,

    /// Node has finished processing step
    ProcessStepFinished,
    /// There are no more request queued
    EndOfRequests,
    /// There are no more responses queued
    EndOfResponses,
    /// There are no more messages queued
    EndOfMessages,

    UpdateModel(Model),

    QueryRequest(Query),
    QueryResponse(QueryProduct),

    /// Request all data from the node
    DataRequestAll,
    /// Request selected data from the node
    DataRequestSelect(Vec<Address>),
    /// Response containing the requested data
    // DataResponse(Vec<(Address, Var)>),
    DataResponse(FnvHashMap<(EntityName, CompName, VarName), Var>),

    /// Request pulling the provided data
    DataPullRequest(Vec<(Address, Var)>),

    EntityCompStateRequest(EntityId, CompName),
    EntityCompStateResponse(StringId),
    // /// External command to be executed on a node
    // #[cfg(feature = "machine")]
    // ExecuteExtCmd((ExecutionContext, ExtCommand)),
    // /// Central-external command to be executed on central
    #[cfg(feature = "machine")]
    ExecuteCentralExtCmd(CentralRemoteCommand),
    // ExecuteCentralExtCmd((ExecutionContext, CentralRemoteCommand)),
    // #[cfg(feature = "machine")]
    // ExecuteCentralExtCmds(Vec<(ExecutionContext, CentralRemoteCommand)>),
}
