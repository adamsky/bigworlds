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
use crate::net::{CompositeAddress, ConnectionOrAddress, Encoding, Transport};
use crate::rpc::leader::{Request, Response};
use crate::util::Shutdown;
use crate::util_net::{decode, encode};
use crate::worker::{WorkerExec, WorkerId};
use crate::{net, rpc, string, worker, EntityId, Model, QueryProduct};

const LEADER_ADDRESS: &str = "0.0.0.0:5912";

pub type LeaderId = Uuid;

/// Single worker as seen by the leader.
#[derive(Clone)]
pub struct Worker {
    pub id: WorkerId,

    pub entities: Vec<EntityId>,
    // pub executor: Option<LocalExec<RequestLocal, Result<Response>>>,
    /// Relays information about worker synchronization situation. Workers with
    /// attached servers can block processing of further steps if any of their
    /// connected clients blocks.
    pub is_blocking_step: bool,
    pub furthest_agreed_step: usize,

    exec: WorkerExec,
}

impl Worker {
    pub fn new(id: WorkerId, exec: WorkerExec) -> Self {
        Self {
            id,
            entities: vec![],
            is_blocking_step: false,
            furthest_agreed_step: 0,
            exec,
        }
    }
}

#[async_trait::async_trait]
impl Executor<rpc::worker::Request, rpc::worker::Response> for Worker {
    async fn execute(&self, req: rpc::worker::Request) -> CoreResult<rpc::worker::Response> {
        match &self.exec {
            WorkerExec::Remote(remote_exec) => remote_exec.execute(req).await?,
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
pub struct State {
    /// Starting configuration.
    pub config: Config,

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
pub struct Config {
    /// Perform automatic stepping through the simulation, initiated on the
    /// leader level.
    pub autostep: Option<std::time::Duration>,

    /// Policy for entity distribution. Most policies involve a dynamic process
    /// of reassigning entities between workers to optimize for different
    /// factors.
    pub distribution: DistributionPolicy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            autostep: None,
            distribution: DistributionPolicy::Random,
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
pub struct Handle {
    pub ctl: LocalExec<rpc::leader::RequestLocal, Result<rpc::leader::Response>>,

    /// Worker executor for running requests coming from a local worker
    pub worker_exec:
        LocalExec<(WorkerId, rpc::leader::RequestLocal), Result<rpc::leader::Response>>,
    // pub worker_id: Option<WorkerId>,
}

#[async_trait::async_trait]
impl Executor<rpc::leader::Request, Result<rpc::leader::Response>> for Handle {
    async fn execute(&self, req: rpc::leader::Request) -> Result<Result<rpc::leader::Response>> {
        self.ctl
            .execute(rpc::leader::RequestLocal::Request(req))
            .await
            .map_err(|e| e.into())
    }
}

impl Handle {
    /// Connects leader to worker. It also makes sure to connect worker
    /// to leader. Local channel communications are more difficult and
    /// we need to set things up both ways.
    pub async fn connect_to_worker(
        &mut self,
        worker_handle: &worker::Handle,
        duplex: bool,
    ) -> Result<()> {
        // connect leader to worker
        self.ctl
            .execute(rpc::leader::RequestLocal::ConnectToWorker(
                worker_handle.leader_exec.clone(),
                self.worker_exec.clone(),
            ))
            .await??;

        if duplex {
            // connect worker to leader
            worker_handle.connect_to_leader(self).await?;
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
    config: Config,
    runtime: runtime::Handle,
    mut shutdown: Shutdown,
) -> Result<Handle> {
    // controller requests
    let (local_ctl_executor, mut local_ctl_stream) = LocalExec::new(20);

    // network requests
    let (net_exec, mut net_stream) = LocalExec::new(20);

    // local worker requests
    let (local_worker_executor, mut local_worker_stream) = LocalExec::new(20);

    net::spawn_listeners(
        listeners,
        net_exec.clone(),
        runtime.clone(),
        shutdown.clone(),
    )?;

    let autostep = config.autostep.clone();
    let mut state = State {
        config,
        clock: 0,
        model: None,
        workers: Default::default(),
        workers_by_addr: Default::default(),
    };
    let manager = manager::spawn(state)?;

    let _ctl_exec = local_ctl_executor.clone();
    runtime.clone().spawn(async move {

        // auto-step task
        if let Some(autostep_delta) = autostep {
            runtime.spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                loop {
                    tokio::time::sleep(autostep_delta).await;
                    match _ctl_exec.execute(rpc::leader::Request::Step.into()).await {
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
        }

        loop {
            let runtime = runtime.clone();
            let manager = manager.clone();

            tokio::select! {
                Some((req, s)) = local_ctl_stream.next() => {
                    debug!("leader: processing local controller request");
                    runtime.clone().spawn(async move {
                        let resp = handle_local_controller_request(req, manager, runtime.clone()).await;
                        s.send(resp);
                    });
                },
                Some(((worker_id, req), s)) = local_worker_stream.next() => {
                    trace!("leader: processing message from local worker");
                    runtime.clone().spawn(async move {
                        let resp = handle_local_worker_request(req, worker_id, manager, runtime.clone()).await;
                        s.send(resp);
                    });

                }
                Some(((maybe_con, bytes), s)) = net_stream.next() => {

                    runtime.clone().spawn(async move {
                        let req: Request = match decode(&bytes, Encoding::Bincode) {
                            Ok(r) => r,
                            Err(e) => {
                                error!("failed decoding request (bincode)");
                                return;
                            }
                        };
                        debug!("leader: processing request from network: {req:?}");
                        let resp = handle_network_request(req, manager.clone(), maybe_con, runtime.clone()).await;
                        debug!("leader: response: {resp:?}");
                        s.send(encode(resp, Encoding::Bincode).unwrap()).unwrap();
                    });
                }
                _ = shutdown.recv() => break,
            }
        }
    });

    Ok(Handle {
        ctl: local_ctl_executor,
        worker_exec: local_worker_executor,
    })
}

async fn handle_local_worker_request(
    req: rpc::leader::RequestLocal,
    worker_id: WorkerId,
    manager: ManagerExec,
    runtime: runtime::Handle,
) -> Result<rpc::leader::Response> {
    use rpc::leader::{RequestLocal, Response};
    match req {
        RequestLocal::ConnectAndRegisterWorker(executor) => {
            let worker = Worker::new(worker_id, WorkerExec::Local(executor));

            // TODO: rework; we don't really need to get information about
            // other workers
            let resp = manager.execute(manager::Request::GetWorkers).await;

            match resp {
                Ok(resp) => match resp {
                    Ok(resp) => {
                        if let manager::Response::Workers(workers) = resp {
                            trace!(
                                "leader: ConnectAndRegisterWorker: current number of workers: {}",
                                workers.len()
                            );
                        } else {
                            unimplemented!()
                        };

                        manager
                            .execute(manager::Request::AddWorker(worker))
                            .await
                            .unwrap();

                        Ok(Response::Empty)
                    }
                    Err(e) => {
                        error!("{}", e.to_string());
                        Err(e)
                    }
                },
                Err(e) => {
                    error!("{}", e.to_string());
                    Err(e.into())
                }
            }
        }
        RequestLocal::Request(req) => {
            debug!("req: {:?}", req);
            handle_worker_request(req, worker_id, manager, runtime).await
        }
        _ => todo!(),
    }
}

pub async fn handle_worker_request(
    req: rpc::leader::Request,
    worker_id: WorkerId,
    leader: ManagerExec,
    runtime: runtime::Handle,
) -> Result<rpc::leader::Response> {
    use rpc::leader::{Request, Response};

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
            let model = leader.get_model().await?;
            Ok(Response::Model(model))
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
        Request::ReadyUntil(target_clock) => {
            unimplemented!();
            // if let Some(worker) = leader.lock().await.workers.get_mut(&worker_id) {
            //     worker.furthest_agreed_step = target_clock;
            // } else {
            //     unimplemented!()
            // }
            Ok(Response::Empty)
        }
        _ => handle_request(req, leader, runtime).await,
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
                .execute(rpc::worker::RequestLocal::IntroduceLeader(worker_leader))
                .await?;
            resp.and_then(|r| {
                if let rpc::worker::Response::Empty = r {
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

async fn handle_controller_request(
    req: rpc::leader::Request,
    leader: ManagerExec,
    runtime: runtime::Handle,
) -> Result<rpc::leader::Response> {
    trace!("leader: processing controller request: {req:?}");
    match req {
        // Pull in a new project into the cluster and load it as the current
        // project, replacing the last project.
        Request::PullModel(model) => {
            leader.pull_model(model).await?;
            Ok(rpc::leader::Response::Empty)
        }
        // Initialize the cluster using provided scenario. Scenario must be
        // present in the currently loaded project.
        Request::Initialize { scenario } => {
            let model = leader.get_model().await?;
            initialize(model, leader, runtime).await?;
            Ok(rpc::leader::Response::Empty)
        }
        // Initialize processing a single step.
        Request::Step => {
            process_step(leader, runtime).await?;
            Ok(rpc::leader::Response::Empty)
        }
        Request::ConnectToWorker(address) => {
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
        _ => handle_request(req, leader, runtime).await,
    }
}

/// Handler containing additional net caller information.
async fn handle_network_request(
    req: rpc::leader::Request,
    manager: ManagerExec,
    maybe_con: ConnectionOrAddress,
    runtime: runtime::Handle,
) -> Result<rpc::leader::Response> {
    match req {
        Request::IntroduceWorker(id) => {
            let exec = match maybe_con {
                ConnectionOrAddress::Connection(con) => WorkerExec::Remote(RemoteExec::new(con)),
                ConnectionOrAddress::Address(_) => unimplemented!(),
            };
            manager.add_worker(id, exec).await?;
            Ok(Response::Empty)
        }
        _ => handle_request(req, manager, runtime).await,
    }
}

async fn handle_request(
    req: rpc::leader::Request,
    manager: ManagerExec,
    runtime: runtime::Handle,
) -> Result<rpc::leader::Response> {
    match req {
        Request::Status => todo!(),
        Request::ConnectToWorker(address) => {
            let bind = SocketAddr::from_str("0.0.0.0:0").unwrap();
            // let endpoint = quinn::Endpoint::client(a).unwrap();
            trace!("worker: connecting to leader: {:?}", address);
            let endpoint = net::quic::make_client_endpoint_insecure(bind).unwrap();
            let connection = endpoint
                .connect(address.address.try_into().unwrap(), "any")
                .unwrap()
                .await
                .unwrap();
            trace!(
                "leader: connected to worker at: {}",
                connection.remote_address()
            );

            let exec = RemoteExec::new(connection);
            exec.execute(rpc::worker::Request::ConnectToLeader(None))
                .await?;

            let worker = Worker {
                // HACK
                id: Uuid::nil(),
                exec: WorkerExec::Remote(exec),
                entities: vec![],
                is_blocking_step: false,
                furthest_agreed_step: 0,
            };

            Ok(Response::Empty)
        }
        Request::PullModel(model) => todo!(),
        Request::Initialize { scenario } => todo!(),
        Request::Step => todo!(),
        Request::Ping(vec) => todo!(),
        Request::Clock => Ok(Response::Clock(manager.get_clock().await?)),
        Request::Model => todo!(),
        Request::ReadyUntil(_) => todo!(),
        Request::SpawnEntity { name, prefab } => {
            use rand::{rng, rngs::StdRng, seq::IteratorRandom, SeedableRng};

            // spawn new entity on random worker
            let workers = manager.get_workers().await?;
            println!("workers: {}", workers.len());
            if let Some((uuid, worker)) = workers.iter().choose(&mut StdRng::from_os_rng()) {
                worker
                    .execute(rpc::worker::Request::SpawnEntity { name, prefab })
                    .await?;
            }

            Ok(Response::Empty)
        }
        Request::GetWorkers => {
            let _workers = manager.get_workers().await?;
            let mut workers = FnvHashMap::default();
            for (worker_id, worker) in _workers {
                match worker.execute(rpc::worker::Request::GetListeners).await? {
                    rpc::worker::Response::GetListeners(listeners) => {
                        workers.insert(worker_id, listeners)
                    }
                    _ => continue,
                };
            }
            Ok(Response::GetWorkers(workers))
        }
        Request::IntroduceWorker(uuid) => unreachable!(),
    }
}

async fn process_step(leader: ManagerExec, runtime: runtime::Handle) -> Result<()> {
    // first wait for all workers to be ready to go to next step
    let workers = leader.get_workers().await?;

    let mut joins = Vec::new();
    for (worker_id, worker) in &workers {
        joins.push(async move {
            let resp = worker
                .execute(rpc::worker::Request::IsBlocking { wait: true })
                .await;

            if let Ok(rpc::worker::Response::IsBlocking(false)) = resp {
                return;
            } else {
                error!("{:?}", resp);
                return;
            }
        });
    }
    futures::future::join_all(joins).await;

    // all workers are ready, broadcast the step request
    let mut joins = Vec::new();
    for (worker_id, worker) in workers.clone() {
        let h = runtime.spawn(async move {
            let resp = worker.execute(rpc::worker::Request::Step).await;
            if let Ok(rpc::worker::Response::Step) = resp {
                return;
            } else {
                error!("{:?}", resp);
                return;
            }
        });
        joins.push(h);
    }
    futures::future::join_all(joins).await;

    // finally increment the clock
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
    trace!(
        "leader: initializing, connected workers: {:?}",
        leader.get_workers().await?.into_keys()
    );

    // first set the new model across the cluster
    leader.set_model(model.clone()).await?;

    // initialize workers
    let workers = leader.get_workers().await?;
    for (_, worker) in workers {
        worker.execute(rpc::worker::Request::Initialize).await?;
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
    /// Random distribution
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
