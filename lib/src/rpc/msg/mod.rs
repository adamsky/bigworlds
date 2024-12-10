//! Message definitions.

#![allow(unused)]

pub mod client_server;

pub use client_server::*;

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::convert::{TryFrom, TryInto};
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;

use fnv::FnvHashMap;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use serde_bytes::serialize;
use serde_repr::*;

use crate::error::Error;
use crate::net::Encoding;
use crate::query::{Query, QueryProduct};
use crate::{EntityId, EntityName, Float, Int, Result, Var};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    ErrorResponse(String),

    PingRequest(Vec<u8>),
    PingResponse(Vec<u8>),

    EntityListRequest,
    EntityListResponse(Vec<EntityName>),

    RegisterClientRequest(RegisterClientRequest),
    RegisterClientResponse(RegisterClientResponse),

    StatusRequest(StatusRequest),
    StatusResponse(StatusResponse),

    AdvanceRequest(AdvanceRequest),
    AdvanceResponse(AdvanceResponse),

    QueryRequest(Query),
    QueryResponse(QueryProduct),

    SpawnEntitiesRequest(SpawnEntitiesRequest),
    SpawnEntitiesResponse(SpawnEntitiesResponse),

    DataPullRequest(DataPullRequest),
    DataPullResponse(DataPullResponse),
    TypedDataPullRequest(TypedDataPullRequest),
    TypedDataPullResponse(TypedDataPullResponse),

    DataTransferRequest(DataTransferRequest),
    DataTransferResponse(DataTransferResponse),
    TypedDataTransferRequest(TypedDataTransferRequest),
    TypedDataTransferResponse(TypedDataTransferResponse),

    ScheduledDataTransferRequest(ScheduledDataTransferRequest),

    ExportSnapshotRequest(ExportSnapshotRequest),
    ExportSnapshotResponse(ExportSnapshotResponse),

    /// Upload a project as tarball. Server must explicitly enable this option
    /// as it's disabled by default.
    UploadProjectArchiveRequest(UploadProjectRequest),
    UploadProjectArchiveResponse(UploadProjectResponse),

    ListScenariosRequest(ListScenariosRequest),
    ListScenariosResponse(ListScenariosResponse),
    LoadScenarioRequest(LoadScenarioRequest),
    LoadScenarioResponse(LoadScenarioResponse),
}

// /// Enumeration of all available message types.
// #[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive, Deserialize_repr,
// Serialize_repr)] #[repr(u8)]
// pub enum MessageType {
//     PingRequest,
//     PingResponse,
//
//     RegisterClientRequest,
//     RegisterClientResponse,
//
//     IntroduceCoordRequest,
//     IntroduceCoordResponse,
//     IntroduceWorkerToCoordRequest,
//     IntroduceWorkerToCoordResponse,
//
//     ExportSnapshotRequest,
//     ExportSnapshotResponse,
//
//     RegisterRequest,
//     RegisterResponse,
//
//     StatusRequest,
//     StatusResponse,
//
//     NativeQueryRequest,
//     NativeQueryResponse,
//     QueryRequest,
//     QueryResponse,
//
//     DataTransferRequest,
//     DataTransferResponse,
//     TypedDataTransferRequest,
//     TypedDataTransferResponse,
//
//     JsonPullRequest,
//     JsonPullResponse,
//     DataPullRequest,
//     DataPullResponse,
//     TypedDataPullRequest,
//     TypedDataPullResponse,
//
//     ScheduledDataTransferRequest,
//     ScheduledDataTransferResponse,
//
//     TurnAdvanceRequest,
//     TurnAdvanceResponse,
//
//     SpawnEntitiesRequest,
//     SpawnEntitiesResponse,
// }
//
// /// Self-described message structure wrapping a byte payload.
// #[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
// pub struct Message {
//     /// Integer identifier allowing for custom message filtering
//     pub task_id: TaskId,
//     /// Describes what is stored within the payload
//     pub type_: MessageType,
//     /// Byte representation of the message payload
//     #[serde(with = "serde_bytes")]
//     pub payload: Vec<u8>,
// }
//
impl Message {
    //     /// Creates a complete `Message` from a payload struct.
    //     pub fn from_payload<P>(payload: P, encoding: &Encoding) ->
    // Result<Message>     where
    //         P: Clone + Serialize + Payload,
    //     {
    //         let msg_type = payload.type_();
    //         let bytes = encode(payload, encoding)?;
    //         Ok(Message {
    //             task_id: 0,
    //             type_: msg_type,
    //             payload: bytes,
    //         })
    //     }
    //
    /// Deserializes from bytes.
    pub fn from_bytes(mut bytes: Vec<u8>, encoding: &Encoding) -> Result<Message> {
        let msg = decode(&bytes, encoding)?;
        Ok(msg)
    }
    //
    /// Serializes into bytes.
    pub fn to_bytes(mut self, encoding: &Encoding) -> Result<Vec<u8>> {
        Ok(bincode::serialize(&self).unwrap())
        // unimplemented!()
        // Ok(encode(self, encoding)?)
    }

    //     /// Unpacks message payload into a payload struct of provided type.
    //     pub fn unpack_payload<'de, P: Payload + Deserialize<'de>>(
    //         &'de self,
    //         encoding: &Encoding,
    //     ) -> Result<P> {
    //         let unpacked = decode(&self.payload, encoding)?;
    //         Ok(unpacked)
    //     }
}
//
// pub trait Payload: Clone {
//     /// Allows payload message structs to state their message type.
//     fn type_(&self) -> MessageType;
// }

// /// Version of the `Var` struct used for untagged ser/deser.
// #[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
// #[serde(untagged)]
// pub enum VarJson {
//     String(String),
//     Int(Int),
//     Float(Float),
//     Bool(bool),
//     Byte(u8),
//     List(Vec<VarJson>),
//     Grid(Vec<Vec<VarJson>>),
//     Map(BTreeMap<VarJson, VarJson>),
// }
//
// impl Eq for VarJson {}
//
// impl Ord for VarJson {
//     fn cmp(&self, other: &Self) -> Ordering {
//         self.partial_cmp(other).unwrap_or(Ordering::Equal)
//     }
// }
//
// impl From<Var> for VarJson {
//     fn from(var: Var) -> Self {
//         match var {
//             // Var::String(v) => VarJson::String(v),
//             Var::Int(v) => VarJson::Int(v),
//             Var::Float(v) => VarJson::Float(v),
//             Var::Bool(v) => VarJson::Bool(v),
//             Var::Byte(v) => VarJson::Byte(v),
//             _ => unimplemented!(),
//         }
//     }
// }
// impl Into<Var> for VarJson {
//     fn into(self) -> Var {
//         match self {
//             // VarJson::String(v) => Var::String(v),
//             VarJson::Int(v) => Var::Int(v),
//             VarJson::Float(v) => Var::Float(v),
//             VarJson::Bool(v) => Var::Bool(v),
//             VarJson::Byte(v) => Var::Byte(v),
//             _ => unimplemented!(),
//         }
//     }
// }

/// Unpacks object from bytes based on selected encoding.
pub fn decode<'de, P: Deserialize<'de>>(bytes: &'de [u8], encoding: &Encoding) -> Result<P> {
    let unpacked = match encoding {
        Encoding::Bincode => bincode::deserialize(bytes)?,
        Encoding::MsgPack => {
            #[cfg(not(feature = "msgpack_encoding"))]
            panic!("trying to unpack using msgpack encoding, but msgpack_encoding crate feature is not enabled");
            #[cfg(feature = "msgpack_encoding")]
            {
                use rmp_serde::config::StructMapConfig;
                let mut de = rmp_serde::Deserializer::new(bytes).with_binary();
                Deserialize::deserialize(&mut de)?
            }
        }
        Encoding::Json => {
            #[cfg(not(feature = "json_encoding"))]
            panic!("trying to unpack using json encoding, but json_encoding crate feature is not enabled");
            #[cfg(feature = "json_encoding")]
            {
                serde_json::from_slice(bytes)?
            }
        }
    };
    Ok(unpacked)
}
