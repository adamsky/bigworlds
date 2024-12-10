#![allow(unused)]

mod manager;

use std::borrow::Borrow;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::ops::DerefMut;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use std::{io, thread};

use fnv::FnvHashMap;
use id_pool::IdPool;
use tokio::runtime;
use tokio::sync::oneshot::Sender;
use tokio::sync::{oneshot, Mutex};
use tokio_stream::StreamExt;
use uuid::Uuid;

// use bigworlds::msg::{Message, MessageType};
use crate::error::{Error as CoreError, Error, Result as CoreResult, Result};
use crate::executor::{Executor, LocalExec, RemoteExec};
use crate::leader::manager::ManagerExec;
use crate::net::{CompositeAddress, Encoding, Transport};
use crate::util::Shutdown;
use crate::util_net::{decode, encode};
use crate::worker::WorkerId;
use crate::{net, rpc, string, EntityId, Model, QueryProduct, WorkerHandle};

const LEADER_ADDRESS: &str = "0.0.0.0:5912";

pub type LeaderId = Uuid;

/// Single worker as seen by the leader.
#[derive(Clone)]
pub struct Worker {
    pub entities: Vec<EntityId>,
    // pub executor: Option<LocalExec<RequestLocal, Result<Response>>>,
    /// Relays information about worker synchronization situation. Workers
    /// that are also servers can block processing of further steps if any of
    /// their connected clients blocks.
    pub is_blocking_step: bool,
    pub furthest_agreed_step: usize,

    exec: WorkerExec,
}

#[derive(Clone)]
pub enum WorkerExec {
    /// Remote executor for sending requests to worker over the wire
    Remote(RemoteExec<rpc::worker::Request, rpc::worker::Response>),
    /// Local executor for sending requests to worker within the same runtime
    Local(LocalExec<rpc::worker::RequestLocal, Result<rpc::worker::Response>>),
}

#[async_trait::async_trait]
impl Executor<rpc::worker::Request, rpc::worker::Response> for Worker {
    async fn execute(&self, req: rpc::worker::Request) -> CoreResult<rpc::worker::Response> {
        match &self.exec {
            WorkerExec::Remote(remote_exec) => remote_exec.execute(req).await,
            WorkerExec::Local(local_exec) => local_exec
                .execute(rpc::worker::RequestLocal::Request(req))
                .await
                .map_err(|e| CoreError::Other(e.to_string()))?
                .map_err(|e| CoreError::Other(e.to_string())),
        }
    }
}

/// Leader holds simulation's central authority struct and manages
/// a network of workers.
///
/// It doesn't hold any entity state, leaving that entirely to workers.
pub struct Leader {
    /// Starting configuration.
    pub config: LeaderConfig,

    /// Map of connected workers by their id.
    pub workers: FnvHashMap<WorkerId, Worker>,
    /// Map of worker addresses to worker ids.
    pub workers_by_addr: FnvHashMap<SocketAddr, WorkerId>,

    /// Currently loaded model.
    pub model: Option<Model>,

    /// Current simulation clock.
    pub clock: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaderConfig {
    /// Policy for entity distribution. Some policies involve a dynamic process
    /// of reassigning entities between workers to optimize for different
    /// factors.
    pub distribution: DistributionPolicy,

    /// Number of required worker connections until initialization can be
    /// carried out.
    ///
    /// # Configuration
    ///
    /// This number can be configured to fit the needs of any particular
    /// deployment. For example a heavy multi-part snapshot of a large
    /// simulation cluster of 1000 workers will necessitate recreating the
    /// cluster before initializing the simulation, as 100 or 500 could not
    /// enough to handle the load (especially if it was really memory-heavy).
    ///
    /// Since the leader doesn't hold any entity data and is unable to
    /// initialize simulation on it's own, the smallest value for this setting
    /// is 1.
    // TODO more detailed initialization requirements could be implemented
    // worker count is not the only way to measure cluster capability,
    // it could be represented as a collection of things like memory capacity,
    // processing power, worker count, etc.
    pub init_req_worker_count: usize,
}

impl Default for LeaderConfig {
    fn default() -> Self {
        Self {
            distribution: DistributionPolicy::MaxSpeed,
            init_req_worker_count: 1,
        }
    }
}

/// Execution handle for sending requests to leader.
///
/// # Worker identification
///
/// The executor implementation for this type must send a `worker_id` to
/// identify with leader. This type stores a `worker_id` to use for
/// subsequent requests.
///
/// We can also supply any other `worker_id` but then we must go through
/// the `worker_exec` and not the executor implemented directly on
/// `LeaderHandle`.
pub struct LeaderHandle {
    pub ctl: LocalExec<rpc::leader::RequestLocal, Result<rpc::leader::Response>>,

    /// Worker executor for running requests coming from a local worker
    pub worker_exec:
        LocalExec<(Option<WorkerId>, rpc::leader::RequestLocal), Result<rpc::leader::Response>>,
    pub worker_id: Option<WorkerId>,
}

#[async_trait::async_trait]
impl Executor<rpc::leader::Request, Result<rpc::leader::Response>> for LeaderHandle {
    async fn execute(&self, req: rpc::leader::Request) -> Result<Result<rpc::leader::Response>> {
        debug!("leaderhandle execute worker_id: {:?}", self.worker_id);
        self.worker_exec
            .execute((self.worker_id, rpc::leader::RequestLocal::Request(req)))
            .await
            .map_err(|e| e.into())
    }
}

impl LeaderHandle {
    /// Connects leader to worker. It also makes sure to connect worker
    /// to leader. Local channel communications are more difficult and
    /// we need to set things up both ways.
    pub async fn connect_to_worker(&mut self, worker_handle: &WorkerHandle) -> Result<()> {
        // connect leader to worker
        if let rpc::leader::Response::Empty = self
            .ctl
            .execute(rpc::leader::RequestLocal::ConnectToWorker(
                worker_handle.leader_exec.clone(),
                self.worker_exec.clone(),
            ))
            .await??
        {
            println!(">>>> leader responded EMPTY/OK to connecttoworker");
        }

        // connect worker to leader
        // if let protocols::worker::Response::ConnectToLeader { worker_id } = worker_handle
        let resp = worker_handle
            .ctl_exec
            .execute(rpc::worker::RequestLocal::ConnectToLeader(
                self.worker_exec.clone(),
                worker_handle.leader_exec.clone(),
            ))
            .await??;
        if let rpc::worker::Response::ConnectToLeader { worker_id } = resp {
            println!(">>>> worker responded: I connected to leader: worker_id {worker_id}");
            self.worker_id = Some(worker_id);
        } else {
            panic!("bad response: {}", resp);
        }

        Ok(())
    }
}

/// Spawns a leader task on the provided runtime.
///
/// Leader serves as cluster's central authority and manages a network
/// of workers. It orchestrates centralized operations such as entity spawning
/// and distribution.
///
/// # Interfaces
///
/// Leader provides a standard network interface for workers to connect
/// to.
///
/// Leader also exposes a worker executor that can be used by local worker
/// tasks.
pub fn spawn(
    listeners: Vec<CompositeAddress>,
    config: LeaderConfig,
    runtime: runtime::Handle,
    mut shutdown: Shutdown,
) -> Result<LeaderHandle> {
    // controller requests channel
    let (mut local_ctl_sender, local_ctl_receiver) = tokio::sync::mpsc::channel::<(
        rpc::leader::RequestLocal,
        tokio::sync::oneshot::Sender<Result<rpc::leader::Response>>,
    )>(20);
    let mut local_ctl_stream = tokio_stream::wrappers::ReceiverStream::new(local_ctl_receiver);
    let mut local_ctl_executor = LocalExec::new(local_ctl_sender);

    // network requests channel
    let (mut net_sender, net_receiver) = tokio::sync::mpsc::channel::<(
        (SocketAddr, Vec<u8>),
        tokio::sync::oneshot::Sender<Vec<u8>>,
    )>(20);
    let mut net_stream = tokio_stream::wrappers::ReceiverStream::new(net_receiver);
    let net_exec = LocalExec::new(net_sender);

    // local worker requests channel
    let (mut local_worker_sender, local_worker_receiver) = tokio::sync::mpsc::channel::<(
        (Option<WorkerId>, rpc::leader::RequestLocal),
        tokio::sync::oneshot::Sender<Result<rpc::leader::Response>>,
    )>(20);
    let mut local_worker_stream =
        tokio_stream::wrappers::ReceiverStream::new(local_worker_receiver);
    let local_worker_executor = LocalExec::new(local_worker_sender);

    net::spawn_listeners(
        listeners,
        net_exec.clone(),
        runtime.clone(),
        shutdown.clone(),
    )?;

    let mut leader = Leader {
        config,
        clock: 0,
        model: None,
        workers: Default::default(),
        workers_by_addr: Default::default(),
    };
    let leader = manager::spawn(leader)?;

    let _ctl_exec = local_ctl_executor.clone();
    runtime.clone().spawn(async move {

        // processing loop
        // TODO: we should probably allow auto-processing to be toggled on or off
        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            loop {
                tokio::time::sleep(Duration::from_micros(50)).await;
                match _ctl_exec.execute(rpc::leader::RequestLocal::Request(rpc::leader::Request::Step)).await {
                    Ok(_) => {
                        debug!("leader processed step");
                        continue
                    },
                    Err(e) => {
                        match e {
                            // 
                            Error::TokioOneshotRecvError(e) => return,
                            _ => {
                                error!("leader failed processing step: {:?}", e);
                                continue
                            },
                        }
                    },
                }
            }
        });

        loop {
            let runtime = runtime.clone();
            let leader = leader.clone();

            tokio::select! {
                Some((req, s)) = local_ctl_stream.next() => {
                    debug!("leader: processing controller request");
                    runtime.clone().spawn(async move {
                        let resp = handle_local_controller_request(req, leader, runtime.clone()).await;
                        s.send(resp);
                    });
                },
                Some(((addr, bytes), s)) = net_stream.next() => {
                    debug!("leader: processing message from network");

                    runtime.clone().spawn(async move {
                        // trace!("message bytes: {:?}", bytes);

                        unimplemented!();
                        // let worker_id = leader.lock().await.workers_by_addr.get(&addr).cloned();
                        // #[cfg(feature = "archive")]
                        // {
                        //     trace!("using archive encoding");
                        //     let req: msg::worker_leader::Request = unsafe { rkyv::from_bytes_unchecked(&bytes).unwrap() };
                        //     trace!("req: {:?}", req);
                        //     let resp = handle_worker_request(req, worker_id, leader, runtime.clone()).await.unwrap();
                        //     let resp = rkyv::to_bytes::<_, 1024>(resp).unwrap();
                        //     s.send(resp.to_vec());
                        // }
                        // #[cfg(not(feature = "archive"))]
                        // {
                        //     // use bincode instead
                        //     trace!("using bincode encoding");
                        //
                        //     let req: msg::worker_leader::Request = bincode::deserialize(&bytes).unwrap();
                        //     trace!("req: {:?}", req);
                        //     // tokio::time::sleep(Duration::from_secs(1)).await;
                        //     let resp = handle_worker_request(req, worker_id, leader).await.unwrap();
                        //     let resp = bincode::serialize(&resp).unwrap();
                        //     s.send(resp.to_vec());
                        // }
                    });
                }
                Some(((worker_id, req), s)) = local_worker_stream.next() => {
                    use rpc::leader::{Response, RequestLocal};
                    trace!("leader: processing message from local worker");
                    let req: RequestLocal = req;
                    match req {
                        RequestLocal::ConnectAndRegisterWorker(executor) => {
                            debug!("handshake");
                            let worker = Worker {
                                exec: WorkerExec::Local(executor),
                                entities: vec![],
                                is_blocking_step: false,
                                furthest_agreed_step: 0,
                            };
                            let id = Uuid::new_v4();

                            let resp = leader.execute(manager::Request::GetWorkers).await;

                            match resp {
                                Ok(resp) => match resp {
                                    Ok(resp) => {
                                        let workers = if let manager::Response::Workers(w) = resp { w } else { unimplemented!() };

                                        if workers.is_empty() {
                                            // leader.lock().await.workers.insert(id, worker);
                                            leader.execute(manager::Request::InsertWorker(id, worker)).await.unwrap();
                                        }

                                        s.send(Ok(Response::Register { worker_id: id }));
                                    },
                                    Err(e) => {
                                        error!("{}", e.to_string());
                                        s.send(Err(e));
                                    },
                                },
                                Err(e) => {
                                    error!("{}", e.to_string());
                                    s.send(Err(e.into()));
                                }
                            }


                            // if let Some(model) = &leader.model {
                            // }

                        }
                        RequestLocal::Request(req) => {
                            debug!("req: {:?}", req);
                            let resp = handle_worker_request(req, worker_id, &leader).await;
                            s.send(resp);
                        }
                        _ => todo!()

                    }
                }
                _ = shutdown.recv() => break,
            }
        }
    });

    Ok(LeaderHandle {
        ctl: local_ctl_executor,
        worker_exec: local_worker_executor,
        worker_id: None,
    })
}

pub async fn handle_worker_request(
    req: rpc::leader::Request,
    worker_id: Option<WorkerId>,
    leader: &ManagerExec,
) -> Result<rpc::leader::Response> {
    use rpc::leader::{Request, Response};

    let worker_id = if let Request::Register(token) = req {
        unimplemented!()
    } else {
        worker_id.ok_or(Error::WorkerNotRegistered("".to_string()))?
    };

    match req {
        Request::PullModel(model) => {
            debug!(
                "pulling model, bytes: {}",
                bincode::serialize(&model)?.len()
            );
            unimplemented!();
            // leader.lock().await.project = Some(project);
            Ok(rpc::leader::Response::PullModel)
        }
        Request::Ping(bytes) => Ok(Response::Ping(bytes)),
        Request::Model => {
            unimplemented!();
            // if let Some(model) = &leader.model {
            //     Ok(Response::Model(model.clone()))
            // }
        }
        // Request::MemorySize => {
        //     println!("memory size unknown");
        // }
        // Request::Entities => {
        //     let mut entities = vec![];
        //     for (worker_id, worker) in &leader.workers {
        //         entities.extend(worker.entities.iter());
        //     }
        //     s.send(Ok(Response::Entities {
        //         machined: entities,
        //         non_machined: vec![],
        //     }));
        // }
        Request::Clock => {
            // println!("leader returning clock to worker: {}", leader.clock);
            // println!("locked?: {:?}", leader.try_lock().is_ok());
            // Ok(Response::Clock(leader.lock().await.clock))
            Ok(Response::Clock(leader.get_clock().await?))
        }
        Request::ReadyUntil(target_clock) => {
            unimplemented!();
            // if let Some(worker) = leader.lock().await.workers.get_mut(&worker_id) {
            //     worker.furthest_agreed_step = target_clock;
            // } else {
            //     unimplemented!()
            // }
            Ok(Response::Empty)
        }
        Request::Register(_) => {
            debug!(">> request: register");
            unimplemented!()
        }
        _ => todo!(),
    }
}

async fn handle_local_controller_request(
    req: rpc::leader::RequestLocal,
    leader: ManagerExec,
    _runtime: runtime::Handle,
) -> Result<rpc::leader::Response> {
    match req {
        rpc::leader::RequestLocal::ConnectToWorker(leader_worker, worker_leader) => {
            let mut resp = leader_worker
                .execute(rpc::worker::RequestLocal::ConnectLeader(worker_leader))
                .await?;
            resp.and_then(|r| {
                if let rpc::worker::Response::Connect = r {
                    // TODO
                    Ok(rpc::worker::Response::Empty)
                } else {
                    Err(Error::UnexpectedResponse("".to_string()))
                }
            })
            .map(|_| rpc::leader::Response::Empty)
            .map_err(|e| Error::FailedConnectingLeaderToWorker(e.to_string()))
        }
        rpc::leader::RequestLocal::Request(req) => {
            handle_controller_request(req, leader, _runtime).await
        }
        _ => todo!(),
    }
}

/// Controller sends requests to be executed by the controller.
async fn handle_controller_request(
    req: rpc::leader::Request,
    leader: ManagerExec,
    runtime: runtime::Handle,
) -> Result<rpc::leader::Response> {
    trace!("leader: processing controller request");
    match req {
        // Pull in a new project into the cluster and load it as the current
        // project, replacing the last project.
        rpc::leader::Request::PullModel(model) => {
            leader.pull_model(model).await?;
            Ok(rpc::leader::Response::Empty)
        }
        // Initialize the cluster using provided scenario. Scenario must be
        // present in the currently loaded project.
        rpc::leader::Request::Initialize { scenario } => {
            println!("initializing");
            let model = leader.get_model().await?;
            initialize(model, leader, runtime).await?;
            Ok(rpc::leader::Response::Empty)
        }
        // Initialize processing a single step.
        rpc::leader::Request::Step => {
            trace!("leader start processing step");
            process_step(leader, runtime).await?;
            Ok(rpc::leader::Response::Empty)
        }
        rpc::leader::Request::ConnectToWorker { address } => {
            unimplemented!()
            // let bind = SocketAddr::from_str("0.0.0.0:0").unwrap();
            // // let endpoint = quinn::Endpoint::client(a).unwrap();
            // trace!("worker: connecting to leader: {:?}", address);
            // let endpoint = net::quic::make_client_endpoint_insecure(bind).unwrap();
            // let connection = endpoint
            //     .connect(address.address.try_into().unwrap(), "any")
            //     .unwrap()
            //     .await
            //     .unwrap();
            // trace!("worker: connected to leader");
            //
            // let leader_exec =
            //     RemoteExec::<worker_leader::Request, worker_leader::Response>::new(connection)
            //         .await
            //         .unwrap();
            // trace!("worker: remote executor created");
            //
            // let leader_exec = leader_exec.clone();
            // // get the data on the leader, e.g. it's Uuid which is provided by the leader itself
            // let req = worker_leader::Request::Introduction;
            // let resp = leader_exec.execute(req).await.unwrap();
            // trace!("worker: sent introduction: got response: {resp:?}");
            //
            // if let worker_leader::Response::Introduction(uuid) = resp {
            //     // store the leader executor so that we can send data to the leader
            //     worker.leader = Some(Leader::Remote(leader_exec));
            // } else {
            //     unimplemented!("unexpected response: {:?}", resp);
            // }
            //
            // // everything went well, return empty response
            // let resp = protocols::worker::Response::Empty;
            // // println!("response: {:?}", resp);
            // sender.send(Ok(resp));
        }
        _ => unimplemented!("{:?}", req),
    }
}

async fn process_step(leader: ManagerExec, runtime: runtime::Handle) -> Result<()> {
    // first wait for all workers to be ready to go to next step
    let workers = leader.get_workers().await?;

    let mut joins = Vec::new();
    for (worker_id, worker) in &workers {
        joins.push(async move {
            match &worker.exec {
                WorkerExec::Remote(_) => unimplemented!(),
                WorkerExec::Local(le) => {
                    let resp = le
                        .execute(rpc::worker::RequestLocal::Request(
                            rpc::worker::Request::IsBlocking { wait: true },
                        ))
                        .await
                        .unwrap();
                    if let Ok(rpc::worker::Response::IsBlocking(false)) = resp {
                        return;
                    } else {
                        error!("{:?}", resp);
                        return;
                    }
                }
            }
        });
    }
    futures::future::join_all(joins).await;
    // warn!("all workers ready!");

    let mut joins = Vec::new();
    for (worker_id, worker) in workers.clone() {
        let h = runtime.spawn(async move {
            match &worker.exec {
                WorkerExec::Remote(_) => unimplemented!(),
                WorkerExec::Local(le) => {
                    let resp = le
                        .execute(rpc::worker::RequestLocal::Request(
                            rpc::worker::Request::Step,
                        ))
                        .await
                        .unwrap();
                    if let Ok(rpc::worker::Response::Step) = resp {
                        return;
                    } else {
                        error!("{:?}", resp);
                        return;
                    }
                }
            }
        });
        joins.push(h);
    }
    futures::future::join_all(joins).await;

    leader.execute(manager::Request::IncrementClock).await?;

    Ok(())
}

async fn initialize_with_scenario(
    scenario: &str,
    leader: ManagerExec,
    runtime: runtime::Handle,
) -> Result<()> {
    // Generate simulation model for selected scenario.
    let model = leader.get_model().await?;
    initialize(model, leader, runtime).await?;
    Ok(())
}

/// Initializes cluster with the provided model.
async fn initialize(model: Model, leader: ManagerExec, runtime: runtime::Handle) -> Result<()> {
    println!(">>> leader: initialize");

    // first set the leader model
    leader.set_model(model.clone()).await?;

    // initialize workers
    let workers = leader.get_workers().await?;
    for (_, worker) in workers {
        worker
            .execute(rpc::worker::Request::Initialize {
                model: model.clone(),
                mod_script: true,
            })
            .await?;
    }

    Ok(())
}

/// Entity distribution policy.
///
/// # Runtime optimization
///
/// Some policies define a more rigid distribution, while others work by
/// actively monitoring the situation across different nodes and transferring
/// entities around as needed.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum DistributionPolicy {
    /// Set binding to a specific node
    BindToWorker(WorkerId),
    // /// Set binding to a specific node based on parameters
    // // TODO what parameters
    // BindToNodeWithParams(String),
    /// Random distribution using an RNG
    Random,
    /// Optimize for processing speed, e.g. using the most capable nodes first
    MaxSpeed,
    /// Optimize for lowest network traffic, grouping together entities
    /// that tend to cause most inter-machine chatter
    LowTraffic,
    /// Balanced approach, sane default policy for most cases
    Balanced,
    /// Focus on similar memory usage across nodes, relative to capability
    SimilarMemoryUsage,
    /// Focus on similar processor usage across nodes, relative to capability
    SimilarProcessorUsage,
    /// Spatial distribution using an octree for automatic subdivision of
    /// space to be handled by different workers.
    ///
    /// # Details
    ///
    /// Works with entities that have a `position` component attached. Uses
    /// x, y and z coordinates of an entity and a tree of octant nodes
    /// representing spatial bounds of different workers to assign the entity
    /// to matching worker. In other words, entities are distributed based on
    /// which "worker box" they are currently in.
    // TODO make it into an engine feature, along with position component
    Spatial,
}
