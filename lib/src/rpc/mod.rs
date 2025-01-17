//! Defines RPC protocols for communication with different bigworlds
//! constructs.

use crate::{leader::LeaderId, worker::WorkerId};

pub mod leader;
pub mod node;
pub mod server;
pub mod worker;

#[cfg(feature = "machine")]
pub mod machine;
pub mod behavior;

pub mod compat;
pub mod msg;

pub enum Caller {
    Leader(LeaderId),
    Worker(WorkerId),
}

/// Describes additional information about a procedure call.
///
/// Can be included alongside a regular request to provide information about
/// the caller, the relay chain of the request as it went through the network,
/// etc.
pub struct Context {
    pub caller: Caller,
}
