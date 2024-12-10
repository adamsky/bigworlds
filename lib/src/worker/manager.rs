//! Worker manager task holds the worker state and shares it via
//! message passing.

use std::fmt::{Display, Formatter};

use fnv::FnvHashMap;
use tokio::sync::{mpsc, oneshot, watch};

use crate::executor::LocalExec;
use crate::server::ServerId;
use crate::worker::part::Part;
use crate::worker::{Leader, Server, ServerExec, Worker};
use crate::{EntityName, Error, Executor, Query, QueryProduct, Result};

pub type ManagerExec = LocalExec<Request, Result<Response>>;

impl ManagerExec {
    pub async fn process_query(&self, query: Query) -> Result<QueryProduct> {
        let resp = self.execute(Request::ProcessQuery(query)).await??;
        if let Response::ProcessQuery(product) = resp {
            Ok(product)
        } else {
            unimplemented!()
        }
    }

    pub async fn get_leader(&self) -> Result<Leader> {
        let resp = self.execute(Request::GetLeader).await??;
        if let Response::GetLeader(leader) = resp {
            Ok(leader)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_leader(&self, leader: Option<Leader>) -> Result<()> {
        let resp = self.execute(Request::SetLeader(leader)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            unimplemented!()
        }
    }

    pub async fn memory_size(&self) -> Result<usize> {
        let resp = self.execute(Request::MemorySize).await??;
        if let Response::MemorySize(size) = resp {
            Ok(size)
        } else {
            unimplemented!()
        }
    }

    pub async fn get_blocked_watch(&self) -> Result<watch::Receiver<bool>> {
        let resp = self.execute(Request::GetBlockedWatch).await??;
        if let Response::GetBlockedWatch(watch) = resp {
            Ok(watch)
        } else {
            unimplemented!()
        }
    }

    pub async fn set_blocked_watch(&self, value: bool) -> Result<()> {
        let resp = self.execute(Request::SetBlockedWatch(value)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            unimplemented!()
        }
    }

    pub async fn get_clock_watch(&self) -> Result<watch::Receiver<usize>> {
        let resp = self.execute(Request::GetClockWatch).await??;
        if let Response::GetClockWatch(watch) = resp {
            Ok(watch)
        } else {
            unimplemented!()
        }
    }

    pub async fn set_clock_watch(&self, value: usize) -> Result<()> {
        let resp = self.execute(Request::SetClockWatch(value)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            unimplemented!()
        }
    }

    pub async fn get_servers(&self) -> Result<FnvHashMap<ServerId, Server>> {
        let resp = self.execute(Request::GetServers).await??;
        if let Response::GetServers(servers) = resp {
            Ok(servers)
        } else {
            unimplemented!()
        }
    }

    pub async fn insert_server(&self, server_id: ServerId, server: Server) -> Result<()> {
        let resp = self
            .execute(Request::InsertServer(server_id, server))
            .await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            unimplemented!()
        }
    }

    pub async fn set_part(&self, part: Part) -> Result<()> {
        let resp = self.execute(Request::SetPart(part)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            unimplemented!()
        }
    }

    // pub async fn node_step(&self, exec: LocalExec<Signal, Result<Signal>>) -> Result<()> {
    //     let resp = self.execute(Request::NodeStep(exec)).await??;
    //     if let Response::Empty = resp {
    //         Ok(())
    //     } else {
    //         unimplemented!()
    //     }
    // }
}

pub fn spawn(mut worker: Worker) -> Result<ManagerExec> {
    let (snd, mut rcv) = mpsc::channel::<(Request, oneshot::Sender<Result<Response>>)>(32);
    let exec = LocalExec::new(snd);
    tokio::spawn(async move {
        while let Some((req, snd)) = rcv.recv().await {
            let resp = handle_request(req, &mut worker).await;
            snd.send(resp);
        }
    });
    Ok(exec)
}

async fn handle_request(req: Request, mut worker: &mut Worker) -> Result<Response> {
    println!(">> worker manager handling request");
    match req {
        Request::GetLeader => {
            println!(">> getleader");
            Ok(Response::GetLeader(worker.leader.clone().ok_or(
                Error::LeaderNotConnected(format!("worker: {}", worker.id)),
            )?))
        }
        Request::SetLeader(leader) => {
            worker.leader = leader;
            Ok(Response::Empty)
        }
        Request::InsertServer(server_id, server) => {
            worker.servers.insert(server_id, server);
            Ok(Response::Empty)
        }
        Request::GetBlockedWatch => Ok(Response::GetBlockedWatch(worker.blocked_watch.1.clone())),
        Request::SetBlockedWatch(value) => {
            worker.blocked_watch.0.send(value);
            Ok(Response::Empty)
        }
        Request::GetClockWatch => Ok(Response::GetClockWatch(worker.clock_watch.1.clone())),
        Request::SetClockWatch(value) => {
            worker.clock_watch.0.send(value);
            Ok(Response::Empty)
        }
        Request::GetServers => Ok(Response::GetServers(worker.servers.clone())),
        Request::GetEntities => {
            if let Some(node) = &worker.part {
                Ok(Response::Entities(
                    node.entities
                        .iter()
                        .map(|(k, _)| k.to_owned())
                        .collect::<Vec<EntityName>>(),
                ))
            } else {
                Err(Error::WorkerNotInitialized(
                    "node object is none".to_string(),
                ))
            }
        }
        Request::MemorySize => {
            if let Some(sim) = &worker.part {
                unimplemented!();
            } else {
                Err(Error::LeaderNotInitialized(format!(
                    "sim part not available"
                )))
            }
        }
        Request::GetClock => {
            unimplemented!();
            // snd.send(Response::Clock(worker.clock));
        }
        Request::ProcessQuery(query) => {
            println!(">> processquery");
            if let Some(part) = &mut worker.part {
                let product = crate::query::process_query(&query, part).await?;
                Ok(Response::ProcessQuery(product))
                // unimplemented!()
            } else {
                Err(Error::WorkerNotInitialized(format!(
                    "node not available, was simulation model loaded onto the cluster?"
                )))
            }
        }
        Request::SetPart(part) => {
            worker.part = Some(part);
            Ok(Response::Empty)
        }
        Request::SpawnMachine => {
            unimplemented!();
            // let handle = machine::spawn(
            // worker.machine_pipe.clone(),
            // tokio::runtime::Handle::current(),
            // )?;
            // if let Some(part) = &mut worker.part {
            // part.machines.insert("_machine_ent".to_string(), vec![]);
            // }
            // Ok(Response::MachineHandle(handle))
        } // Request::NodeStep(exec) => {
          //     if let Some(ref mut node) = &mut worker.node {
          //         // node.step(exec).await?;
          //     }
          //     Ok(Response::Empty)
          // }
    }
}

#[derive(Clone)]
pub enum Request {
    GetClock,
    GetBlockedWatch,
    SetBlockedWatch(bool),
    GetClockWatch,
    SetClockWatch(usize),
    GetServers,
    GetEntities,
    MemorySize,
    GetLeader,
    SetLeader(Option<Leader>),
    InsertServer(ServerId, Server),
    ProcessQuery(Query),
    SetPart(Part),
    SpawnMachine,
    // NodeStep(LocalExec<Signal, Result<Signal>>),
}

#[derive(Clone)]
pub enum Response {
    GetBlockedWatch(watch::Receiver<bool>),
    GetClockWatch(watch::Receiver<usize>),
    GetServers(FnvHashMap<ServerId, Server>),
    GetLeader(Leader),
    MemorySize(usize),
    ProcessQuery(QueryProduct),
    Entities(Vec<EntityName>),
    Empty,
}

impl Display for Response {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            _ => unimplemented!(),
        }
    }
}
