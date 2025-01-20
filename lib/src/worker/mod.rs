pub mod config;
pub mod manager;
pub mod part;

use std::io::{ErrorKind, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tokio::runtime;
use tokio::sync::oneshot::Sender;
use tokio::sync::{oneshot, watch, Mutex};
use tokio::task::JoinSet;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{StreamExt, StreamMap};

use deepsize::DeepSizeOf;
use fnv::FnvHashMap;
use id_pool::IdPool;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::Entity;
use crate::error::{Error as CoreError, Error, Result as CoreResult};
use crate::executor::{Executor, LocalExec, RemoteExec};
use crate::net::{CompositeAddress, ConnectionOrAddress, Encoding, Transport};
use crate::query::{self, process_query};
use crate::rpc::compat::{
    DataPullRequest, DataPullResponse, DataTransferRequest, DataTransferResponse, PullRequestData,
    TransferResponseData, VarSimDataPack,
};
use crate::server::{self, ServerId};
use crate::util::Shutdown;
use crate::util_net::{decode, encode};
use crate::{
    leader, net, rpc, string, Address, CompName, EntityId, EntityName, Model, Query, QueryProduct,
    Result, StringId, Var, VarType,
};

pub use crate::worker::manager::ManagerExec;
pub use config::Config;

use part::Part;

#[derive(Clone)]
pub enum WorkerExec {
    /// Remote executor for sending requests to worker over the wire
    Remote(RemoteExec<rpc::worker::Request, Result<rpc::worker::Response>>),
    /// Local executor for sending requests to worker within the same runtime
    // Local(LocalExec<(Option<ServerId>, rpc::worker::RequestLocal), Result<rpc::worker::Response>>),
    Local(LocalExec<rpc::worker::RequestLocal, Result<rpc::worker::Response>>),
}

/// Network-unique identifier for a single worker
pub type WorkerId = Uuid;

pub struct WorkerState {
    /// Integer identifier unique across the cluster
    pub id: WorkerId,

    /// Worker configuration
    pub config: Config,

    /// List of addresses that can be used to contact this worker.
    pub listeners: Vec<CompositeAddress>,

    /// Simulation model kept up to date with leader.
    pub model: Option<Model>,

    /// Worker-held part of the simulation.
    ///
    /// # Initialization
    ///
    /// Worker can exist within a cluster without being initialized with
    /// a simulation model. For this reason the `part` field is an optional
    /// type.
    pub part: Option<Part>,

    /// Cluster leader as seen by the worker.
    ///
    /// # Leader loss and re-election
    ///
    /// Leader can run on any of the cluster nodes, it's just another task
    /// to be spawned on the runtime. Nodes have a mechanism for collectively
    /// choosing one of them to spawn a leader.
    ///
    /// Worker must coordinate with it's node in case when
    pub leader: Option<Leader>,

    pub servers: FnvHashMap<ServerId, Server>,

    /// Remote workers from the same cluster as seen by this worker.
    pub remote_workers: FnvHashMap<WorkerId, RemoteWorker>,

    pub blocked_watch: (watch::Sender<bool>, watch::Receiver<bool>),
    pub clock_watch: (watch::Sender<usize>, watch::Receiver<usize>),
}

#[derive(Clone)]
pub struct Leader {
    pub exec: LeaderExec,
    pub worker_id: WorkerId,
    // pub listeners: Vec<CompositeAddress>,
}

#[derive(Clone)]
pub enum LeaderExec {
    /// Remote executor for sending requests to leader over the wire
    Remote(RemoteExec<rpc::leader::Request, Result<rpc::leader::Response>>),
    /// Local executor for sending requests to leader within the same runtime
    Local(LocalExec<(WorkerId, rpc::leader::RequestLocal), Result<rpc::leader::Response>>),
}

#[async_trait::async_trait]
impl Executor<rpc::leader::Request, rpc::leader::Response> for Leader {
    async fn execute(&self, req: rpc::leader::Request) -> CoreResult<rpc::leader::Response> {
        match &self.exec {
            LeaderExec::Remote(remote_exec) => remote_exec.execute(req).await?,
            LeaderExec::Local(local_exec) => local_exec
                .execute((self.worker_id, req.into()))
                .await
                .map_err(|e| CoreError::Other(e.to_string()))?
                .map_err(|e| CoreError::Other(e.to_string())),
        }
    }
}

#[derive(Clone)]
pub struct Server {
    pub exec: ServerExec,
    pub worker_id: Option<WorkerId>,
}

/// Servers are attached to workers to handle distributing the data to
/// clients.
///
/// Worker tracks connected servers. Worker can send requests to connected
/// servers, for example telling them to reconnect to different worker.
#[derive(Clone)]
pub enum ServerExec {
    /// Remote executor for sending requests to leader over the wire
    Remote(RemoteExec<rpc::server::Request, rpc::server::Response>),
    /// Local executor for sending requests to leader within the same runtime
    Local(LocalExec<(Option<WorkerId>, rpc::server::RequestLocal), Result<rpc::server::Response>>),
}

#[async_trait::async_trait]
impl Executor<rpc::server::Request, rpc::server::Response> for Server {
    async fn execute(&self, req: rpc::server::Request) -> CoreResult<rpc::server::Response> {
        match &self.exec {
            ServerExec::Remote(remote_exec) => remote_exec.execute(req).await,
            ServerExec::Local(local_exec) => local_exec
                .execute((self.worker_id, rpc::server::RequestLocal::Request(req)))
                .await
                .map_err(|e| CoreError::Other(e.to_string()))?
                .map_err(|e| CoreError::Other(e.to_string())),
        }
    }
}

// TODO: track additional information about remote workers
#[derive(Clone)]
pub struct RemoteWorker {
    /// Globally unique uuid self-assigned by the remote worker.
    pub id: Uuid,

    pub exec: WorkerExec,
}

#[async_trait::async_trait]
impl Executor<rpc::worker::Request, rpc::worker::Response> for RemoteWorker {
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

#[derive(Clone)]
pub struct Handle {
    // TODO: not needed?
    pub server_addr: Option<SocketAddr>,

    /// Controller executor, allowing control over the worker task
    pub ctl: LocalExec<rpc::worker::RequestLocal, Result<rpc::worker::Response>>,

    /// Server executor for running requests coming from a local server
    pub server_exec: LocalExec<rpc::worker::RequestLocal, Result<rpc::worker::Response>>,

    /// Executor for running requests coming from a local leader
    pub leader_exec: LocalExec<rpc::worker::RequestLocal, Result<rpc::worker::Response>>,

    pub behavior_exec: LocalExec<rpc::worker::Request, Result<rpc::worker::Response>>,
}

impl Handle {
    /// Connect the worker to a local leader task using the provided handle.
    pub async fn connect_to_leader(&self, handle: &leader::Handle) -> Result<()> {
        self.ctl
            .execute(rpc::worker::RequestLocal::ConnectToLeader(
                handle.worker_exec.clone(),
                self.leader_exec.clone(),
            ))
            .await??;

        Ok(())
    }

    /// Connect the worker to another remote worker over a network using the
    /// provided address.
    pub async fn connect_to_worker(&self, address: &str) -> Result<()> {
        self.ctl
            .execute(rpc::worker::RequestLocal::Request(
                rpc::worker::Request::ConnectToWorker(address.parse()?),
            ))
            .await??;

        Ok(())
    }

    /// Connect the worker to another remote worker running on the same
    /// runtime.
    pub async fn connect_to_local_worker(&self, worker_handle: &Handle) -> Result<()> {
        self.ctl
            .execute(rpc::worker::RequestLocal::ConnectToWorker())
            .await??;

        Ok(())
    }

    /// Connect the worker to another remote worker running on the same
    /// runtime.
    pub async fn connect_to_local_server(&self, handle: &server::Handle) -> Result<()> {
        self.ctl
            .execute(rpc::worker::RequestLocal::ConnectToServer(
                None,
                handle.worker.clone(),
                self.server_exec.clone(),
            ))
            .await??;

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.ctl
            .execute(rpc::worker::RequestLocal::Shutdown)
            .await??;
        Ok(())
    }
}

/// Creates a new `Worker` task on the provided runtime.
///
/// # Usage details
///
/// In a simulation cluster made up of multiple machines, there is at least
/// one `Worker` running on each machine.
///
/// In terms of initialization, `Worker`s can either actively reach out to
/// an already existing cluster to join in, or passively wait for incoming
/// connection from a leader.
///
/// Unless configured otherwise, new `Worker`s can dynamically join into
/// already initialized cluster, introducing on-the-fly changes to the
/// cluster composition.
///
/// # Connection management and topology
///
/// `Worker`s are connected to, and orchestrated by, a cluster leader.
/// They are also connected to each other. Connections are either direct,
/// or indirect.
///
/// Indirect connections mean messages being routed between cluster members.
/// Leader keeps workers updated about any changes to the cluster membership.
/// Leader also gives new workers unique ids that are later used for
/// identification across cluster operations.
///
/// # Relay-worker
///
/// Worker can be left stateless and serve as a relay, meaning a cluster
/// participant tasked with forwarding requests.
///
/// Same as a regular stateful worker, relay-worker can be used to back
/// a server that will respond to client queries. Relay-worker-backed servers
/// allow for spreading the load of serving connected clients across more
/// machines, without expanding the core simulation-state-bearing worker base.
///
/// # Local cache
///
/// Worker is able to cache data from responses it gets from other cluster
/// members. Caching behavior can be configured to only allow for up-to-date
/// data to be cached and used for subsequent queries.
pub fn spawn(
    listeners: Vec<CompositeAddress>,
    config: Config,
    runtime: runtime::Handle,
    mut shutdown: Shutdown,
) -> Result<Handle> {
    let (local_ctl_executor, mut local_ctl_stream) = LocalExec::new(20);
    let (local_leader_executor, mut local_leader_stream) = LocalExec::new(20);
    let (local_behavior_executor, mut local_behavior_stream) = LocalExec::new(20);
    let (net_executor, mut net_stream) = LocalExec::new(20);
    let (local_server_executor, mut local_server_stream) = LocalExec::new(20);

    net::spawn_listeners(
        listeners.clone(),
        net_executor.clone(),
        runtime.clone(),
        shutdown.clone(),
    )?;

    let clock = watch::channel(0);
    let blocked = tokio::sync::watch::channel(false);
    let mut state = WorkerState {
        id: Uuid::new_v4(),
        config,
        // TODO: validate that the listener addresses we're passing here were
        // actually valid and actual listeners were started on those
        listeners,
        leader: None,
        remote_workers: FnvHashMap::default(),
        servers: FnvHashMap::default(),
        blocked_watch: blocked,
        clock_watch: clock,
        model: None,
        part: Some(Part::new()),
    };
    // worker state is held by a dedicated manager task
    let manager = manager::spawn(state)?;

    // clones to satisfy borrow checker
    let runtime_c = runtime.clone();
    let local_leader_executor_c = local_leader_executor.clone();
    let local_behavior_executor_c = local_behavior_executor.clone();

    runtime.spawn(async move {
        loop {
            // debug!("worker loop start");
            let runtime = runtime_c.clone();

            tokio::select! {
                Some((req, s)) = local_ctl_stream.next() => {
                    let runtime = runtime.clone();
                    let worker = manager.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    runtime.clone().spawn(async move {
                        debug!("worker: processing controller message");
                        let resp = handle_local_request(req, worker, local_behavior_executor, net_exec, runtime.clone()).await;
                        s.send(resp);
                    });
                },
                Some((req, s)) = local_server_stream.next() => {
                    let req: rpc::worker::RequestLocal = req;
                    debug!("worker: processing message from local server");

                    let worker = manager.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    runtime.clone().spawn(async move {
                        // let resp = handle_local_server_request(req, server_id, worker).await;
                        let resp = handle_local_request(req, worker, local_behavior_executor, net_exec, runtime.clone()).await;
                        s.send(resp);
                    });

                },
                Some((req, s)) = local_leader_stream.next() => {
                    use rpc::worker::{Response, RequestLocal};
                    // worker receives leader executor channel
                    debug!("worker: processing message from local leader");
                    let req: RequestLocal = req;

                    let worker = manager.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    runtime.clone().spawn(async move {
                        let resp = handle_local_request(req, worker, local_behavior_executor, net_exec, runtime.clone()).await;
                        s.send(resp);
                    });

                }
                Some((req, s)) = local_behavior_stream.next() => {
                    let worker = manager.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    runtime.clone().spawn(async move {
                        let resp = handle_request(req,
                            worker,
                            local_behavior_executor,
                            net_exec.clone(),
                            runtime.clone()
                        ).await;
                        s.send(resp);
                    });
                },
                Some(((maybe_con, req), s)) = net_stream.next() => {
                    debug!("worker: processing network message from network");

                    let worker = manager.clone();
                    let local_behavior_executor = local_behavior_executor_c.clone();
                    let net_exec = net_executor.clone();
                    runtime.clone().spawn(async move {
                        let req: rpc::worker::Request = match decode(&req, Encoding::Bincode) {
                            Ok(r) => r,
                            Err(e) => {
                                error!("failed decoding request (bincode)");
                                return;
                            }
                        };
                        println!("decoded request");
                        let resp = handle_net_request(req, worker.clone(), maybe_con, local_behavior_executor, net_exec, runtime.clone()).await;
                        s.send(encode(resp, Encoding::Bincode).unwrap()).unwrap();
                    });
                },
                _ = shutdown.recv() => break,
            }
        }
    });

    Ok(Handle {
        server_addr: None,
        server_exec: local_server_executor,
        ctl: local_ctl_executor,
        leader_exec: local_leader_executor,
        behavior_exec: local_behavior_executor,
    })
}

async fn handle_local_request(
    req: rpc::worker::RequestLocal,
    manager: ManagerExec,
    behavior_exec: LocalExec<
        rpc::worker::Request,
        std::result::Result<rpc::worker::Response, CoreError>,
    >,
    net_exec: LocalExec<(ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    _runtime: runtime::Handle,
) -> Result<rpc::worker::Response> {
    use rpc::worker::RequestLocal;

    match req {
        RequestLocal::ConnectToLeader(_worker_leader, leader_worker) => {
            let mut worker_leader = None;
            let my_id = manager.get_meta().await?;

            _worker_leader
                .execute((
                    my_id,
                    rpc::leader::RequestLocal::ConnectAndRegisterWorker(leader_worker),
                ))
                .await??;

            worker_leader = Some(Leader {
                exec: LeaderExec::Local(_worker_leader),
                worker_id: my_id,
            });
            manager.set_leader(worker_leader).await?;

            Ok(rpc::worker::Response::ConnectToLeader { worker_id: my_id })
        }
        RequestLocal::IntroduceLeader(exec) => {
            log::debug!("worker connecting to leader...");

            let my_id = manager.get_meta().await?;
            let exec = LeaderExec::Local(exec);

            match manager.get_leader().await {
                Ok(mut leader) => {
                    log::debug!("worker already aware of a leader");
                    leader.exec = exec;
                    manager.set_leader(Some(leader)).await;
                }
                Err(_) => {
                    let leader = Leader {
                        exec,
                        worker_id: my_id,
                    };
                    manager.set_leader(Some(leader)).await;
                }
            };

            log::debug!("worker successfuly connected to leader");

            Ok(rpc::worker::Response::Empty)
        }
        RequestLocal::ConnectToServer(server_id, worker_server, server_worker) => {
            let resp = worker_server
                .execute((
                    None,
                    rpc::server::RequestLocal::ConnectAndRegisterWorker(
                        server_id,
                        server_worker.clone(),
                    ),
                ))
                .await??;
            if let rpc::server::Response::Register { worker_id } = resp {
                let server = Server {
                    exec: ServerExec::Local(worker_server),
                    worker_id: Some(worker_id),
                };
                warn!("set the server with worker_id");

                manager.insert_server(Uuid::nil(), server).await?;

                Ok(rpc::worker::Response::ConnectToServer { worker_id })
            } else {
                Err(Error::UnexpectedResponse("".to_string()))
            }
        }
        RequestLocal::ConnectAndRegisterServer(exec) => {
            let server = Server {
                exec: ServerExec::Local(exec),
                worker_id: None,
            };
            warn!("set the server without worker_id");

            let server_id = Uuid::new_v4();
            manager.insert_server(server_id, server).await?;

            Ok(rpc::worker::Response::Register { server_id })
        }
        RequestLocal::Request(req) => {
            handle_request(req, manager, behavior_exec, net_exec, _runtime).await
        }
        RequestLocal::Shutdown => {
            manager.shutdown().await?;
            Ok(rpc::worker::Response::Empty)
        }
        RequestLocal::ConnectToWorker() => todo!(),
    }
}

/// Network-oriented request handler, exposing some additional network-specific
/// context on the incoming request.
async fn handle_net_request(
    req: rpc::worker::Request,
    manager: ManagerExec,
    maybe_con: ConnectionOrAddress,
    behavior_exec: LocalExec<
        rpc::worker::Request,
        std::result::Result<rpc::worker::Response, Error>,
    >,
    net_exec: LocalExec<(ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    _runtime: runtime::Handle,
) -> Result<rpc::worker::Response> {
    use rpc::worker::{Request, Response};
    match req {
        Request::IntroduceWorker(id) => {
            match maybe_con {
                ConnectionOrAddress::Connection(con) => {
                    // re-use the connection if the request was transported
                    // via quic
                    manager
                        .add_remote_worker(RemoteWorker {
                            id,
                            exec: WorkerExec::Remote(RemoteExec::new(con)),
                        })
                        .await?;
                }
                ConnectionOrAddress::Address(addr) => {
                    // as a fallback use the address of the caller known here
                    unimplemented!()
                }
            }

            let my_id = manager.get_meta().await?;

            Ok(Response::IntroduceWorker(my_id))
        }
        _ => handle_request(req, manager, behavior_exec, net_exec, _runtime).await,
    }
}

async fn handle_request(
    req: rpc::worker::Request,
    manager: ManagerExec,
    behavior_exec: LocalExec<
        rpc::worker::Request,
        std::result::Result<rpc::worker::Response, Error>,
    >,
    net_exec: LocalExec<(ConnectionOrAddress, Vec<u8>), Vec<u8>>,
    _runtime: runtime::Handle,
) -> Result<rpc::worker::Response> {
    use rpc::worker::{Request, Response};

    debug!("worker: processing request: {req}");

    match req {
        Request::ConnectToLeader(address) => {
            let connection = match address {
                Some(address) => {
                    let bind = SocketAddr::from_str("0.0.0.0:0").unwrap();
                    // let endpoint = quinn::Endpoint::client(a).unwrap();
                    trace!("worker: connecting to leader: {:?}", address);
                    let endpoint = net::quic::make_client_endpoint_insecure(bind).unwrap();
                    endpoint
                        .connect(address.address.try_into().unwrap(), "any")
                        .unwrap()
                        .await
                        .unwrap()
                }
                None => {
                    // TODO: use the bidir connection that this request arrived with
                    unimplemented!()
                }
            };
            trace!("worker: connected to leader");

            let leader_exec =
                RemoteExec::<rpc::leader::Request, Result<rpc::leader::Response>>::new(
                    connection.clone(),
                );
            trace!("worker: remote executor created");

            let leader_exec = leader_exec.clone();
            let my_id = manager.get_meta().await?;
            // get the data on the leader, e.g. it's Uuid which is provided by the leader itself
            let req = rpc::leader::Request::IntroduceWorker(my_id);
            let resp = leader_exec.execute(req).await??;
            trace!("worker: sent introduction: got response: {resp:?}");

            if let rpc::leader::Response::Empty = resp {
                // store the leader executor so that we can send data to the leader
                // worker.leader = Some(Leader {Remote(leader_exec));
                manager
                    .set_leader(Some(Leader {
                        exec: LeaderExec::Remote(leader_exec),
                        worker_id: my_id,
                    }))
                    .await?;
            } else {
                unimplemented!("unexpected response: {:?}", resp);
            }

            let _runtime_c = _runtime.clone();
            let connection_c = connection.clone();
            _runtime.spawn(async move {
                if let Err(e) = net::quic::handle_connection(
                    net_exec.clone(),
                    connection_c.clone(),
                    _runtime_c.clone(),
                )
                .await
                {
                    error!("connection failed: {reason}", reason = e.to_string())
                }
            });

            Ok(Response::Empty)
        }
        Request::Ping(bytes) => Ok(Response::Ping(bytes)),
        Request::MemorySize => {
            let size = manager.memory_size().await?;
            trace!("worker mem size: {}", size);
            Ok(Response::MemorySize(size))
        }
        Request::IsBlocking { wait } => {
            trace!("received IsBlocking request: wait: {wait}");
            let mut is_blocked = manager.get_blocked_watch().await?.clone();
            if *is_blocked.borrow() == true {
                trace!("worker is blocked, waiting to unblock");
                loop {
                    if *is_blocked.borrow() == false {
                        debug!("worker unblocked");
                        return Ok(rpc::worker::Response::IsBlocking(false));
                    } else {
                        is_blocked.changed().await;
                        // println!("last thing that worker did?");
                    }
                }
            } else {
                trace!("worker is not blocked");
                Ok(Response::IsBlocking(false))
            }
        }
        Request::Step => {
            let current_clock = manager.get_clock_watch().await?.borrow().clone();

            let new_clock = current_clock + 1;
            manager.set_clock_watch(new_clock).await?;
            trace!(">> did set clock watch clock + 1");
            for (server_id, server) in manager.get_servers().await? {
                server
                    .execute(rpc::server::Request::ClockChangedTo(new_clock))
                    .await;
            }
            trace!("send clockchangedto to all servers");

            let event = string::new_truncate("step");

            manager
                .get_behavior_tx()
                .await?
                .send(rpc::behavior::Request::Trigger(event.clone()))
                .inspect_err(|e| warn!("{e}"));

            trace!("worker processed step");
            Ok(Response::Step)
        }
        Request::Initialize => {
            trace!(">>> worker: initializing");

            manager.set_clock_watch(0).await?;

            manager.initialize(behavior_exec.clone()).await?;

            Ok(Response::Empty)
        }

        Request::GetModel => {
            trace!(">>> worker getting current leader");
            let leader = manager.get_leader().await?;
            trace!(">>> worker asking leader for the current model");
            match leader.execute(rpc::leader::Request::Model).await? {
                rpc::leader::Response::Model(model) => Ok(Response::GetModel(model)),
                _ => panic!(),
            }
        }

        Request::PullModel(model) => {
            // propagate request to leader
            let leader = manager.get_leader().await?;
            let resp = leader
                .execute(rpc::leader::Request::PullModel(model))
                .await?;
            Ok(Response::PullProject)
        }
        Request::SetModel(model) => {
            manager.set_model(model).await?;
            Ok(Response::Empty)
        }
        Request::Clock => {
            let leader = manager.get_leader().await?;
            trace!("worker_id: {:?}", leader.worker_id);
            let clock = match leader.execute(rpc::leader::Request::Clock).await? {
                rpc::leader::Response::Clock(clock) => clock,
                rpc::leader::Response::Empty => panic!("got unexpected empty response"),
                _ => panic!("worker failed getting clock value from leader"),
            };
            Ok(Response::Clock(clock))
        }
        Request::Query(query) => {
            let mut product = QueryProduct::Empty;
            match query.scope {
                query::Scope::Global => {
                    trace!("worker: processing global query");

                    // broadcast query

                    let mut workers = manager.get_remote_workers().await?;

                    // no workers are visible but worker is configured to not
                    // rely solely on peer-to-peer worker connections; instead
                    // it will pass the query through the leader
                    // TODO: put this "no-peering" rule into worker config
                    if workers.is_empty() {
                        let leader = manager.get_leader().await?;
                        let resp = leader.execute(rpc::leader::Request::GetWorkers).await?;
                        match resp {
                            rpc::leader::Response::GetWorkers(_workers) => {
                                for (worker_id, worker) in _workers {
                                    //
                                    //
                                    // let remote_worker = RemoteWorker {
                                    //     id: worker_id
                                    //     exec: todo!(),
                                    // };
                                }
                            }
                            _ => unimplemented!(),
                        }
                    }

                    let local = {
                        let query = query.clone();
                        tokio::spawn(async move { manager.process_query(query.clone()).await })
                    };

                    let mut set = JoinSet::new();
                    trace!("worker: query: num of remote workers: {}", workers.len());
                    for (id, worker) in workers {
                        let query = query.clone();
                        set.spawn(async move { worker.execute(Request::Query(query)).await });
                    }
                    while let Some(res) = set.join_next().await {
                        let response = res.map_err(|e| Error::NetworkError(format!("{e}")))??;
                        match response {
                            Response::Query(_product) => product.merge(_product)?,
                            _ => return Err(Error::UnexpectedResponse(response.to_string())),
                        }
                    }

                    let local = local.await.map_err(|e| Error::Other(format!("{e}")))??;
                    trace!("worker: global query: product: {product:?}, local: {local:?}");
                    product.merge(local)?;
                }
                query::Scope::Local => {
                    product = manager.process_query(query).await?;
                }
                _ => unimplemented!(),
            }

            trace!("worker: query: product: {:?}", product);
            Ok(Response::Query(product))
        }
        Request::SetBlocking(blocking) => {
            manager.set_blocked_watch(blocking).await?;
            trace!("set worker blocked watch to {}", blocking);
            Ok(Response::Empty)
        }
        Request::EntityList => {
            let resp = manager.execute(manager::Request::GetEntities).await??;
            if let manager::Response::Entities(entities) = resp {
                Ok(Response::EntityList(entities))
            } else {
                Err(Error::UnexpectedResponse(format!(
                    "expected Response::Entities"
                )))
            }
        }

        Request::Query(query) => Ok(Response::Empty),
        Request::NewRequirements {
            ram_mb,
            disk_mb,
            transfer_mb,
        } => todo!(),
        Request::Entities => todo!(),
        Request::Status => {
            // TODO: contact manager
            Ok(Response::Status {
                uptime: 0,
                worker_count: 1,
            })
        }
        Request::Trigger(_) => todo!(),
        Request::SpawnEntity { name, prefab } => {
            manager.spawn_entity(name, prefab).await?;
            Ok(Response::Empty)
        }
        Request::ConnectToWorker(address) => {
            // received a request to connect to remote worker at given address

            // initiate a new connection to the remote worker
            let connection = net::quic::make_connection(address.address.try_into().unwrap())
                .await
                .map_err(|e| Error::NetworkError(e.to_string()))?;
            let exec = RemoteExec::new(connection);

            // send the introductory message
            let my_id = manager.get_meta().await?;
            let resp = exec
                .execute(rpc::worker::Request::IntroduceWorker(my_id))
                .await??;

            // parse the response and store the remote worker handle
            let worker_id = match resp {
                Response::IntroduceWorker(id) => id,
                _ => unimplemented!(),
            };
            let remote_worker = RemoteWorker {
                id: worker_id,
                exec: WorkerExec::Remote(exec),
            };
            manager.add_remote_worker(remote_worker).await?;

            // at this point we have two or more workers connected, which could
            // constitute a cluster, but we don't know if there's a leader

            // trigger a leader check
            // handle_request(Request::GetLeader, manager, behavior_exec, _runtime).await?;
            match manager.get_leader().await {
                Err(Error::LeaderNotSelected(_)) => {
                    // there's no leader found across all workers

                    // initiate leader election

                    // broadcast
                    let workers = manager.get_remote_workers().await?;

                    unimplemented!();

                    // let _ = manager.elect_leader().await?;
                }
                Ok(leader) => {
                    // leader was found, cluster is functional
                    trace!("leader was found");
                }
                _ => (),
            }

            Ok(Response::Empty)
        }
        Request::IntroduceWorker(uuid) => unreachable!(),
        Request::GetLeader => {
            let leader = manager.get_leader().await?;

            let leader = match leader.exec {
                LeaderExec::Remote(remote) => Some(remote.remote_address()),
                // LeaderExec::Remote(remote_exec) => remote_exec.local_ip(),
                LeaderExec::Local(local_exec) => {
                    // TODO: ask leader for their network listener addresses
                    // panic!("unable to send back leader addr as it's stored as local")
                    None
                }
            };

            Ok(Response::GetLeader(leader))
        }
        Request::GetListeners => Ok(Response::GetListeners(manager.get_listeners().await?)),
        Request::SetVar(address, var) => {
            manager.set_var(address, var).await?;
            Ok(Response::Empty)
        }
        Request::GetVar(address) => Ok(Response::GetVar(manager.get_var(address).await?)),
    }
}
