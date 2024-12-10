//! Leader manager task holds the leader state and shares it via
//! message passing.

use std::fmt::{Display, Formatter};

use fnv::FnvHashMap;
use tokio::sync::{mpsc, oneshot};

use crate::executor::LocalExec;
use crate::leader::{Leader, Worker, WorkerExec};
use crate::worker::WorkerId;
use crate::{Error, Executor, Model, Result};

pub type ManagerExec = LocalExec<Request, Result<Response>>;

impl ManagerExec {
    pub async fn get_clock(&self) -> Result<usize> {
        let resp = self.execute(Request::GetClock).await??;
        if let Response::Clock(clock) = resp {
            Ok(clock)
        } else {
            Err(Error::UnexpectedResponse(format!("")))
        }
    }

    pub async fn get_workers(&self) -> Result<FnvHashMap<WorkerId, Worker>> {
        let resp = self.execute(Request::GetWorkers).await??;
        if let Response::Workers(we) = resp {
            Ok(we)
        } else {
            unimplemented!()
        }
    }

    pub async fn pull_model(&self, model: Model) -> Result<()> {
        let resp = self.execute(Request::PullModel(model)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            unimplemented!()
        }
    }

    pub async fn get_model(&self) -> Result<Model> {
        let resp = self.execute(Request::GetModel).await??;
        if let Response::GetModel(model) = resp {
            Ok(model)
        } else {
            unimplemented!()
        }
    }

    pub async fn set_model(&self, model: Model) -> Result<()> {
        let resp = self.execute(Request::SetModel(model)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            unimplemented!()
        }
    }
}

pub fn spawn(mut leader: Leader) -> Result<ManagerExec> {
    let (snd, mut rcv) = mpsc::channel::<(Request, oneshot::Sender<Result<Response>>)>(32);
    let exec = LocalExec::new(snd);
    tokio::spawn(async move {
        while let Some((req, snd)) = rcv.recv().await {
            let resp = handle_request(req, &mut leader).await;
            snd.send(resp);
        }
    });
    Ok(exec)
}

async fn handle_request(req: Request, mut leader: &mut Leader) -> Result<Response> {
    match req {
        Request::GetClock => Ok(Response::Clock(leader.clock)),
        Request::IncrementClock => {
            leader.clock += 1;
            Ok(Response::Clock(leader.clock))
        }
        Request::GetWorkers => Ok(Response::Workers(leader.workers.clone())),
        Request::WorkerCount => unimplemented!("worker count"),
        Request::InsertWorker(worker_id, worker) => {
            leader.workers.insert(worker_id, worker);
            Ok(Response::Empty)
        }
        Request::PullModel(model) => {
            leader.model = Some(model);
            Ok(Response::Empty)
        }
        Request::GetModel => {
            if let Some(model) = &leader.model {
                Ok(Response::GetModel(model.clone()))
            } else {
                Err(Error::LeaderNotInitialized(format!("model not available")))
            }
        }
        Request::SetModel(model) => {
            leader.model = Some(model);
            Ok(Response::Empty)
        }
    }
}

#[derive(Clone)]
pub enum Request {
    GetClock,
    IncrementClock,
    GetWorkers,
    WorkerCount,
    InsertWorker(WorkerId, Worker),
    PullModel(Model),
    GetModel,
    SetModel(Model),
}

#[derive(Clone)]
pub enum Response {
    Empty,
    Clock(usize),
    Workers(FnvHashMap<WorkerId, Worker>),
    // WorkerExecs(Vec<WorkerExec>),
    WorkerCount(usize),
    GetModel(Model),
}

impl Display for Response {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clock(clock) => write!(f, "Clock"),
            // Self::WorkerExecs(_) => write!(f, "WorkerExecs"),
            _ => unimplemented!(),
        }
    }
}
