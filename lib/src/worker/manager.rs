//! Worker manager task holds the worker state and shares it via
//! message passing.

use std::fmt::{Display, Formatter};
use std::str::FromStr;

use fnv::FnvHashMap;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;

use crate::behavior::BehaviorHandle;
use crate::executor::LocalExec;
use crate::net::CompositeAddress;
use crate::server::ServerId;
use crate::worker::part::Part;
use crate::worker::{Leader, Server, ServerExec, WorkerState};
use crate::{
    net, rpc, Address, EntityName, Error, Executor, Model, Query, QueryProduct, Result, Var,
};

use super::{RemoteWorker, WorkerId};

pub type ManagerExec = LocalExec<Request, Result<Response>>;

impl ManagerExec {
    pub async fn get_meta(&self) -> Result<(WorkerId)> {
        let resp = self.execute(Request::GetMeta).await??;
        if let Response::GetMeta(m) = resp {
            Ok(m)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn process_query(&self, query: Query) -> Result<QueryProduct> {
        let resp = self.execute(Request::ProcessQuery(query)).await??;
        if let Response::ProcessQuery(product) = resp {
            Ok(product)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
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
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_remote_workers(&self) -> Result<FnvHashMap<WorkerId, RemoteWorker>> {
        let resp = self.execute(Request::GetRemoteWorkers).await??;
        if let Response::GetRemoteWorkers(workers) = resp {
            Ok(workers)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn add_remote_worker(&self, worker: RemoteWorker) -> Result<()> {
        let resp = self.execute(Request::AddRemoteWorker(worker)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_listeners(&self) -> Result<Vec<CompositeAddress>> {
        let resp = self.execute(Request::GetListeners).await??;
        if let Response::GetListeners(listeners) = resp {
            Ok(listeners)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_model(&self, model: Model) -> Result<()> {
        let resp = self.execute(Request::SetModel(model)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_var(&self, address: Address) -> Result<Var> {
        Ok(self.execute(Request::GetVar(address)).await??.try_into()?)
    }

    pub async fn set_var(&self, address: Address, var: Var) -> Result<()> {
        Ok(drop(self.execute(Request::SetVar(address, var)).await??))
    }

    pub async fn memory_size(&self) -> Result<usize> {
        let resp = self.execute(Request::MemorySize).await??;
        if let Response::MemorySize(size) = resp {
            Ok(size)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_blocked_watch(&self) -> Result<watch::Receiver<bool>> {
        let resp = self.execute(Request::GetBlockedWatch).await??;
        if let Response::GetBlockedWatch(watch) = resp {
            Ok(watch)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_blocked_watch(&self, value: bool) -> Result<()> {
        let resp = self.execute(Request::SetBlockedWatch(value)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_clock_watch(&self) -> Result<watch::Receiver<usize>> {
        let resp = self.execute(Request::GetClockWatch).await??;
        if let Response::GetClockWatch(watch) = resp {
            Ok(watch)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn set_clock_watch(&self, value: usize) -> Result<()> {
        let resp = self.execute(Request::SetClockWatch(value)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_servers(&self) -> Result<FnvHashMap<ServerId, Server>> {
        let resp = self.execute(Request::GetServers).await??;
        if let Response::GetServers(servers) = resp {
            Ok(servers)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn insert_server(&self, server_id: ServerId, server: Server) -> Result<()> {
        let resp = self
            .execute(Request::InsertServer(server_id, server))
            .await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn initialize(
        &self,
        worker_exe: LocalExec<
            rpc::worker::Request,
            std::result::Result<rpc::worker::Response, Error>,
        >,
    ) -> Result<()> {
        let resp = self.execute(Request::Initialize(worker_exe)).await??;
        if let Response::Empty = resp {
            Ok(())
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn get_behavior_tx(
        &self,
    ) -> Result<tokio::sync::broadcast::Sender<rpc::behavior::Request>> {
        let resp = self.execute(Request::GetBehaviorTx).await??;
        if let Response::GetBehaviorTx(tx) = resp {
            Ok(tx)
        } else {
            Err(Error::UnexpectedResponse(resp.to_string()))
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.execute(Request::Shutdown).await??;
        Ok(())
    }

    pub async fn spawn_entity(&self, name: String, prefab: String) -> Result<()> {
        self.execute(Request::SpawnEntity(name, prefab)).await??;
        Ok(())
    }
}

pub fn spawn(mut worker: WorkerState) -> Result<ManagerExec> {
    use tokio_stream::StreamExt;

    let (exec, mut stream) = LocalExec::new(32);
    tokio::spawn(async move {
        while let Some((req, snd)) = stream.next().await {
            let resp = handle_request(req, &mut worker).await;
            snd.send(resp);
        }
    });
    Ok(exec)
}

async fn handle_request(req: Request, mut worker: &mut WorkerState) -> Result<Response> {
    trace!(">> worker ({}) manager handling request: {req}", worker.id);
    trace!(
        "worker ({}) blocked watch {}",
        worker.id,
        worker.blocked_watch.1.borrow().to_string()
    );
    trace!(
        "worker ({}) clock watch {}",
        worker.id,
        worker.clock_watch.1.borrow().to_string()
    );
    match req {
        Request::GetMeta => Ok(Response::GetMeta(worker.id)),
        Request::GetLeader => {
            if let Some(leader) = &worker.leader {
                // if the leader is known just return the handle
                // TODO: perhaps do a ping request here to make sure we're
                // holding an up-to-date handle? otherwise we trust it's
                // managed properly elsewhere
                println!("got leader locally");
                Ok(Response::GetLeader(leader.clone()))
            } else {
                // if there's no handle present locally, check with the known
                // workers
                println!("no leader locally, querying other workers");
                // TODO: execute in parallel
                let mut set = JoinSet::new();
                for (id, remote) in &worker.remote_workers {
                    let remote = remote.clone();
                    remote.execute(rpc::worker::Request::GetLeader).await?;
                    println!("spawning on set");
                    set.spawn(async move { remote.execute(rpc::worker::Request::GetLeader).await });
                }
                let mut leader_addr = None;
                // TODO: compare responses and confirm they have the same
                // leader
                while let Some(result) = set.join_next().await {
                    println!("got one join");
                    let resp = result.map_err(|e| Error::Other(e.to_string()))??;
                    println!("it finished: {resp:?}");
                    match resp {
                        rpc::worker::Response::GetLeader(addr) => {
                            if let Some(addr) = addr {
                                leader_addr = Some(addr);
                                break;
                            }
                        }
                        _ => unimplemented!(),
                    }
                }
                if let Some(addr) = leader_addr {
                    // connect to leader
                    let bind = std::net::SocketAddr::from_str("0.0.0.0:0").unwrap();
                    let endpoint = net::quic::make_client_endpoint_insecure(bind).unwrap();
                    let connection = endpoint.connect(addr, "any").unwrap().await.unwrap();
                    let leader = Leader {
                        exec: crate::worker::LeaderExec::Remote(crate::executor::RemoteExec::new(
                            connection,
                        )),
                        worker_id: worker.id,
                    };

                    Ok(Response::GetLeader(leader))
                } else {
                    // TODO: elect new leader
                    // panic!("leader not elected")
                    Err(Error::LeaderNotSelected(format!("")))
                }
            }
        }
        Request::SetLeader(leader) => {
            worker.leader = leader;
            println!("worker leader is some? {}", worker.leader.is_some());
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
        Request::GetBehaviorTx => {
            if let Some(part) = &worker.part {
                Ok(Response::GetBehaviorTx(part.behavior_broadcast.clone()))
            } else {
                Err(Error::WorkerNotInitialized(
                    "part object is none".to_string(),
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
        Request::SetModel(model) => {
            worker.model = Some(model);
            Ok(Response::Empty)
        }
        Request::Initialize(exe) => {
            if let Some(model) = &worker.model {
                worker.part = Some(Part::from_model(model, false, exe)?);
                Ok(Response::Empty)
            } else {
                Err(Error::WorkerNotInitialized(format!("model is not set")))
            }
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
        }
        Request::Shutdown => {
            // include sending signal synced procs/machines
            if let Some(part) = &worker.part {
                println!("manager sent shutdown to procs");
                part.behavior_broadcast
                    .send(rpc::behavior::Request::Shutdown)
                    .inspect_err(|e| println!("{e}"));
                Ok(Response::Empty)
            } else {
                Err(Error::WorkerNotInitialized(format!(
                    "can't shutdown items on worker part"
                )))
            }
        }
        Request::SpawnEntity(name, prefab) => {
            if let Some(model) = &worker.model {
                if let Some(ref mut part) = worker.part {
                    use crate::entity::Entity;
                    let entity = Entity::from_prefab_name(prefab, &model)?;
                    part.entities.insert(name, entity);
                    Ok(Response::Empty)
                } else {
                    Err(Error::WorkerNotInitialized(format!(
                        "worker part unavailable"
                    )))
                }
            } else {
                Err(Error::WorkerNotInitialized(format!(
                    "worker model unavailable"
                )))
            }
        }
        Request::GetRemoteWorkers => Ok(Response::GetRemoteWorkers(worker.remote_workers.clone())),
        Request::AddRemoteWorker(remote_worker) => {
            worker
                .remote_workers
                .insert(remote_worker.id, remote_worker);
            Ok(Response::Empty)
        }
        Request::GetListeners => Ok(Response::GetListeners(worker.listeners.clone())),
        Request::SetVar(address, var) => {
            // TODO: expand to possibly set data on remote worker
            if let Some(part) = &mut worker.part {
                if let Some(entity) = part.entities.get_mut(&address.entity) {
                    entity.storage.set_from_var(&address, &var)?;
                }
            }
            Ok(Response::Empty)
        }
        Request::GetVar(address) => {
            if let Some(part) = &worker.part {
                if let Some(entity) = part.entities.get(&address.entity) {
                    Ok(Response::GetVar(
                        entity.storage.get_var(&address.storage_index())?.clone(),
                    ))
                } else {
                    Err(Error::FailedGettingVar(address))
                }
            } else {
                Err(Error::WorkerNotInitialized(format!("part not available")))
            }
        }
    }
}

#[derive(Clone, strum::Display)]
pub enum Request {
    GetMeta,
    GetClock,
    GetBlockedWatch,
    SetBlockedWatch(bool),
    GetClockWatch,
    SetClockWatch(usize),
    GetServers,
    GetRemoteWorkers,
    AddRemoteWorker(RemoteWorker),
    GetEntities,
    GetBehaviorTx,
    MemorySize,
    GetLeader,
    SetLeader(Option<Leader>),
    InsertServer(ServerId, Server),
    ProcessQuery(Query),
    SetModel(Model),
    SetVar(Address, Var),
    GetVar(Address),
    Initialize(LocalExec<rpc::worker::Request, std::result::Result<rpc::worker::Response, Error>>),
    SpawnMachine,
    Shutdown,
    SpawnEntity(String, String),
    GetListeners,
}

#[derive(Clone)]
pub enum Response {
    GetMeta((WorkerId)),
    GetBlockedWatch(watch::Receiver<bool>),
    GetClockWatch(watch::Receiver<usize>),
    GetServers(FnvHashMap<ServerId, Server>),
    GetRemoteWorkers(FnvHashMap<WorkerId, RemoteWorker>),
    GetLeader(Leader),
    GetBehaviorTx(tokio::sync::broadcast::Sender<rpc::behavior::Request>),
    GetListeners(Vec<CompositeAddress>),
    GetVar(Var),
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

impl TryInto<Var> for Response {
    type Error = Error;

    fn try_into(self) -> std::result::Result<Var, Self::Error> {
        match self {
            Response::GetVar(var) => Ok(var),
            _ => Err(Error::UnexpectedResponse(self.to_string())),
        }
    }
}
