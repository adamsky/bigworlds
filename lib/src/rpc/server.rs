use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::server::ServerId;
use crate::worker::WorkerId;
use crate::Result;

use super::{msg::Message, worker};

#[derive(Clone)]
pub enum RequestLocal {
    ConnectToWorker(
        LocalExec<worker::RequestLocal, Result<worker::Response>>,
        LocalExec<(Option<WorkerId>, RequestLocal), Result<Response>>,
    ),
    ConnectAndRegisterWorker(
        Option<ServerId>,
        LocalExec<worker::RequestLocal, Result<worker::Response>>,
    ),
    Request(Request),
}

impl From<Request> for RequestLocal {
    fn from(value: Request) -> Self {
        Self::Request(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, strum::Display)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Request {
    ConnectToWorker { address: CompositeAddress },
    Status,
    // UploadProject(Project),
    Message(Message),

    Redirect,
    ClockChangedTo(usize),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Response {
    Empty,
    // uptime is counted in seconds
    Status { uptime: usize },
    UploadProject { success: bool },
    ConnectToWorker { server_id: ServerId },
    Message(Message),

    Register { worker_id: WorkerId },
    Redirect,
}
