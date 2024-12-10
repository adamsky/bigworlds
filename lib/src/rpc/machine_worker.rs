use bigworlds::core::{EntityId, EntityName};

use crate::executor::LocalExec;
use crate::msg::worker_server;
use crate::server::ServerId;
use crate::worker::WorkerId;
use crate::{Query, QueryProduct, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    Query(Query),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    QueryProduct(QueryProduct),
    Empty,
}
