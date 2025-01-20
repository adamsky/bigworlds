//! Leader manager task holds the leader state and shares it via
//! message passing.

use std::fmt::{Display, Formatter};

use fnv::FnvHashMap;
use tokio::sync::{mpsc, oneshot};

use crate::executor::LocalExec;
use crate::leader::{State, Worker, WorkerExec};
use crate::worker::WorkerId;
use crate::{rpc, Error, Executor, Model, Result};

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
        if let Response::Workers(workers) = resp {
            Ok(workers)
        } else {
            Err(Error::UnexpectedResponse(format!("{resp}")))
        }
    }

    pub async fn add_worker(&self, id: WorkerId, exec: WorkerExec) -> Result<()> {
        let worker = Worker::new(id, exec);
        let resp = self.execute(Request::AddWorker(worker)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(format!("{resp}")))
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
            Err(Error::UnexpectedResponse(resp.to_string()))
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

pub fn spawn(mut leader: State) -> Result<ManagerExec> {
    use tokio_stream::StreamExt;

    let (exec, mut stream) = LocalExec::new(32);
    tokio::spawn(async move {
        while let Some((req, snd)) = stream.next().await {
            let resp = handle_request(req, &mut leader).await;
            snd.send(resp);
        }
    });
    Ok(exec)
}

async fn handle_request(req: Request, mut leader: &mut State) -> Result<Response> {
    match req {
        Request::GetClock => Ok(Response::Clock(leader.clock)),
        Request::IncrementClock => {
            leader.clock += 1;
            Ok(Response::Clock(leader.clock))
        }
        Request::GetWorkers => Ok(Response::Workers(leader.workers.clone())),
        Request::AddWorker(worker) => {
            leader.workers.insert(worker.id, worker);
            Ok(Response::Empty)
        }
        Request::WorkerCount => todo!(),
        Request::GetModel => {
            if let Some(model) = &leader.model {
                Ok(Response::GetModel(model.clone()))
            } else {
                Err(Error::LeaderNotInitialized(format!("model not available")))
            }
        }
        Request::PullModel(model) => {
            // TODO: pulling the model is initiated by workers, perform
            // additional checks if they have permission to do this

            // TODO: should propagating of the model to the workers happen
            // on the manager level?

            leader.model = Some(model.clone());

            // propagate the model to workers
            // TODO: make concurrent
            for (worker_id, worker) in &leader.workers {
                worker
                    .execute(rpc::worker::Request::SetModel(model.clone()))
                    .await?;
            }

            Ok(Response::Empty)
        }
        Request::SetModel(model) => {
            leader.model = Some(model.clone());

            // propagate the model to workers
            // TODO: make concurrent
            for (worker_id, worker) in &leader.workers {
                worker
                    .execute(rpc::worker::Request::SetModel(model.clone()))
                    .await?;
            }
            Ok(Response::Empty)
        }
    }
}

#[derive(Clone)]
pub enum Request {
    GetClock,
    IncrementClock,
    GetWorkers,
    AddWorker(Worker),
    WorkerCount,
    GetModel,
    PullModel(Model),
    SetModel(Model),
}

#[derive(Clone, strum::Display)]
pub enum Response {
    Empty,
    Clock(usize),
    Workers(FnvHashMap<WorkerId, Worker>),
    // WorkerExecs(Vec<WorkerExec>),
    WorkerCount(usize),
    GetModel(Model),
}
