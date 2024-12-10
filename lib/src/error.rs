use std::net::{AddrParseError, SocketAddr, TcpStream};
use std::num::ParseIntError;

use num_enum::TryFromPrimitiveError;
use thiserror::Error;

use crate::entity::StorageIndex;
use crate::net::Transport;
use crate::{Address, CompName, EntityName};

use crate::server::ClientId;

pub type Result<T> = core::result::Result<T, Error>;

/// Enumeration of all possible errors.
#[derive(Error, Clone, Debug, Serialize, Deserialize)]
pub enum Error {
    #[error("unexpected response: {0}")]
    UnexpectedResponse(String),
    #[error("would block")]
    WouldBlock,
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("failed getting client by id: {0}")]
    FailedGettingClientById(ClientId),
    #[error("failed getting client by address: {0}")]
    FailedGettingClientByAddr(SocketAddr),

    #[error("failed conversion: {0}")]
    FailedConversion(String),
    #[error("bincode error: {0}")]
    BincodeError(String),
    #[error("bincode error: {0}")]
    SerdeJsonError(String),

    #[error("other: {0}")]
    Other(String),

    #[error("got error response: {0}")]
    ErrorResponse(String),

    #[error("io error: {0}")]
    IoError(String),
    #[error("failed parsing int: {0}")]
    IntParseError(String),
    #[error("failed parsing address: {0}")]
    AddrParseError(String),
    #[error("transport unavailable: {0}")]
    TransportUnavailable(Transport),

    #[error("no activity for {0} milliseconds, terminating server")]
    ServerKeepaliveLimitReached(u32),
    #[error("worker not connected: {0}")]
    WorkerNotConnected(String),
    #[error("failed connecting server to worker: {0}")]
    FailedConnectingServerToWorker(String),

    #[error("leader not connected: {0}")]
    LeaderNotConnected(String),
    #[error("worker not registered: {0}")]
    WorkerNotRegistered(String),
    #[error("failed registering worker: {0}")]
    FailedRegisteringWorker(String),
    #[error("failed connecting worker to leader: {0}")]
    FailedConnectingWorkerToLeader(String),
    #[error("failed connecting leader to worker: {0}")]
    FailedConnectingLeaderToWorker(String),

    #[error("leader not initialized: {0}")]
    LeaderNotInitialized(String),
    #[error("worker not initialized: {0}")]
    WorkerNotInitialized(String),

    #[error("unknown error")]
    Unknown,

    #[error("would block")]
    NetworkError(String),

    #[error("vfs error: {0}")]
    VfsError(String),

    #[error("yaml deserialization error: {0}")]
    YamlDeserError(String),
    #[error("toml deserialization error: {0}")]
    TomlDeserError(String),
    #[error("semver error: {0}")]
    SemverError(String),
    #[error("parsing error: {0}")]
    ParsingError(String),
    #[error("failed parsing int: {0}")]
    ParseIntError(String),
    #[error("failed parsing float: {0}")]
    ParseFloatError(String),
    #[error("failed parsing bool: {0}")]
    ParseBoolError(String),

    #[error("failed requesting new integer id: no more ids available in the pool?")]
    RequestIdError,
    #[error("failed returning integer id to pool: already exists?")]
    ReturnIdError,

    #[error("invalid var type: {0}")]
    InvalidVarType(String),
    #[error("invalid local address: {0}")]
    InvalidAddress(String),
    #[error("invalid local address: {0}")]
    InvalidLocalAddress(String),

    #[cfg(feature = "lz4")]
    #[error("failed decompressing snapshot: {0}")]
    SnapshotDecompressionError(String),
    #[error("failed reading snapshot header: {0}")]
    FailedReadingSnapshotHeader(String),
    #[error("failed reading snapshot: {0}")]
    FailedReadingSnapshot(String),
    #[error("failed creating snapshot: {0}")]
    FailedCreatingSnapshot(String),

    #[error("failed reading scenario: missing module: {0}")]
    ScenarioMissingModule(String),

    #[error("model: no entity prefab named: {0}")]
    NoEntityPrefab(EntityName),
    #[error("model: no component named: {0}")]
    NoComponentModel(CompName),

    #[error("failed getting entity with id: {0}")]
    FailedGettingEntityById(u32),
    #[error("failed getting entity with name: {0}")]
    FailedGettingEntityByName(String),
    #[error("failed getting variable: {0}")]
    FailedGettingVarFromSim(Address),
    #[error(
    "failed getting variable from entity storage: comp: {}, var: {}",
    _0.0,
    _0.1
    )]
    FailedGettingVarFromEntityStorage(StorageIndex),

    #[error("failed creating address from string: {0}")]
    FailedCreatingAddress(String),
    #[error("failed creating variable from string: {0}")]
    FailedCreatingVar(String),

    #[error("model root not found, searched path: {0}, recursion levels: {1}")]
    ModelRootNotFound(String, usize),

    #[error("required engine feature not available: {0}, required by module: {1}")]
    RequiredEngineFeatureNotAvailable(String, String),

    #[cfg(feature = "machine")]
    #[error("runtime machine panic")]
    MachinePanic(#[from] crate::machine::error::Error),

    #[error("tokio oneshot receive error: {0}")]
    TokioOneshotRecvError(String),

    #[error("timed out")]
    Timeout,
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e.to_string())
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: ParseIntError) -> Self {
        Self::IntParseError(e.to_string())
    }
}

impl From<std::net::AddrParseError> for Error {
    fn from(e: AddrParseError) -> Self {
        Self::AddrParseError(e.to_string())
    }
}

impl From<Box<bincode::ErrorKind>> for Error {
    fn from(e: Box<bincode::ErrorKind>) -> Self {
        Self::BincodeError(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::SerdeJsonError(e.to_string())
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(e: serde_yaml::Error) -> Self {
        Self::YamlDeserError(e.to_string())
    }
}

impl From<semver::Error> for Error {
    fn from(e: semver::Error) -> Self {
        Self::SemverError(e.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Self::TomlDeserError(e.to_string())
    }
}

impl From<std::num::ParseFloatError> for Error {
    fn from(e: std::num::ParseFloatError) -> Self {
        Self::ParseFloatError(e.to_string())
    }
}

impl From<std::str::ParseBoolError> for Error {
    fn from(e: std::str::ParseBoolError) -> Self {
        Self::ParseBoolError(e.to_string())
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for Error {
    fn from(e: tokio::sync::oneshot::error::RecvError) -> Self {
        Self::TokioOneshotRecvError(e.to_string())
    }
}

impl From<vfs::VfsError> for Error {
    fn from(e: vfs::VfsError) -> Self {
        Self::VfsError(e.to_string())
    }
}
