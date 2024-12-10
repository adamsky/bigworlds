mod manager;
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
use crate::leader::LeaderHandle;
use crate::net::{CompositeAddress, Encoding, Transport};
use crate::query::process_query;
use crate::rpc::compat::{
    DataPullRequest, DataPullResponse, DataTransferRequest, DataTransferResponse, PullRequestData,
    TransferResponseData, VarSimDataPack,
};
use crate::server::ServerId;
use crate::util::Shutdown;
use crate::util_net::{decode, encode};
use crate::{
    net, rpc, string, Address, CompName, EntityId, EntityName, Model, Query, QueryProduct, Result,
    StringId, Var, VarType,
};

use crate::worker::manager::ManagerExec;

use part::Part;

/// Network-unique identifier for a single worker
pub type WorkerId = Uuid;

pub struct Worker {
    /// Integer identifier unique across the cluster
    pub id: WorkerId,

    /// Worker configuration
    pub config: WorkerConfig,

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

    pub blocked_watch: (watch::Sender<bool>, watch::Receiver<bool>),
    pub clock_watch: (watch::Sender<usize>, watch::Receiver<usize>),
}

#[derive(Clone)]
pub struct Leader {
    pub exec: LeaderExec,
    pub worker_id: Option<WorkerId>,
}

#[derive(Clone)]
pub enum LeaderExec {
    /// Remote executor for sending requests to leader over the wire
    Remote(RemoteExec<rpc::leader::Request, rpc::leader::Response>),
    /// Local executor for sending requests to leader within the same runtime
    Local(LocalExec<(Option<WorkerId>, rpc::leader::RequestLocal), Result<rpc::leader::Response>>),
}

#[async_trait::async_trait]
impl Executor<rpc::leader::Request, rpc::leader::Response> for Leader {
    async fn execute(&self, req: rpc::leader::Request) -> CoreResult<rpc::leader::Response> {
        match &self.exec {
            LeaderExec::Remote(remote_exec) => remote_exec.execute(req).await,
            LeaderExec::Local(local_exec) => local_exec
                .execute((self.worker_id, rpc::leader::RequestLocal::Request(req)))
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

pub struct WorkerConfig {
    pub addr: Option<String>,

    // TODO consider max CPU usage and how it could be calculated
    /// Maximum allowed memory use
    pub max_ram_mb: usize,
    /// Maximum allowed disk use
    pub max_disk_mb: usize,
    /// Maximum allowed network transfer use
    pub max_transfer_mb: usize,

    // TODO overhaul the authentication system
    /// Whether the worker uses a password to authorize connecting comrade
    /// workers
    pub use_auth: bool,
    /// Password used for incoming connection authorization
    pub passwd_list: Vec<String>,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            addr: None,
            max_ram_mb: 0,
            max_disk_mb: 0,
            max_transfer_mb: 0,
            use_auth: false,
            passwd_list: vec![],
        }
    }
}

#[derive(Clone)]
pub struct WorkerHandle {
    // TODO not needed?
    pub server_addr: Option<SocketAddr>,

    /// Controller executor, allowing control over the worker task
    pub ctl_exec: LocalExec<rpc::worker::RequestLocal, Result<rpc::worker::Response>>,

    /// Server executor for running requests coming from a local server
    pub server_exec:
        LocalExec<(Option<ServerId>, rpc::worker::RequestLocal), Result<rpc::worker::Response>>,

    /// Executor for running requests coming from a local leader
    pub leader_exec: LocalExec<rpc::worker::RequestLocal, Result<rpc::worker::Response>>,

    pub processor_exec: LocalExec<rpc::worker::Request, Result<rpc::worker::Response>>,
}

impl WorkerHandle {
    /// See leader::LeaderHandle::connect_to_worker
    pub async fn connect_to_leader(&self, leader_handle: &LeaderHandle) -> Result<()> {
        unimplemented!()
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
/// Worker can be left stateless and serve as a relay, a cluster participant
/// tasked first and foremost with forwarding signals.
///
/// Same as a regular stateful worker, relay-worker can be used to back
/// a server that will respond to client queries. Relay-worker servers allow
/// for spreading the load of serving connected clients across more machines,
/// without expanding the core stateful worker base.
///
/// # Local cache
///
/// Worker is able to cache data from responses it gets from other cluster
/// members. Caching behavior can be configured to only allow for up-to-date
/// data to be cached and used for subsequent queries.
///
/// # Workplace
///
/// "Worker spawner" mode called `Workplace` allows for instantiating multiple
/// workers within a context of a single CLI application, based on incoming
/// leaders' requests. This could make it easier for people to share
/// their machines with people who want to run simulations. For safety reasons
/// it would make sense to allow running it in "sandbox" mode, with only the
/// runtime-level logic enabled.
///
/// # Thread-per-core
///
/// Worker abstraction could work well with a "thread per core" strategy. This
/// means there would be a single worker per every machine core, instead of
/// single worker per machine utilizing multiple cores with thread-pooling.
/// "Thread per core" promises performance improvements caused by reducing
/// expensive context switching operations. It would require having the ability
/// to switch `SimNode`s to process entities in a single-threaded fashion.
pub fn spawn(
    listeners: Vec<CompositeAddress>,
    config: WorkerConfig,
    runtime: runtime::Handle,
    mut shutdown: Shutdown,
) -> Result<WorkerHandle> {
    // controller requests channel
    let (mut local_ctl_sender, local_ctl_receiver) = tokio::sync::mpsc::channel::<(
        rpc::worker::RequestLocal,
        tokio::sync::oneshot::Sender<Result<rpc::worker::Response>>,
    )>(20);
    let mut local_ctl_stream = tokio_stream::wrappers::ReceiverStream::new(local_ctl_receiver);
    let mut local_ctl_executor = LocalExec::new(local_ctl_sender);

    // local leader requests channel
    let (mut local_leader_sender, local_leader_receiver) = tokio::sync::mpsc::channel::<(
        rpc::worker::RequestLocal,
        tokio::sync::oneshot::Sender<Result<rpc::worker::Response>>,
    )>(20);
    let mut local_leader_stream =
        tokio_stream::wrappers::ReceiverStream::new(local_leader_receiver);
    let mut local_leader_executor = LocalExec::new(local_leader_sender);

    // local processor requests channel
    let (mut local_processor_sender, local_processor_receiver) = tokio::sync::mpsc::channel::<(
        rpc::worker::Request,
        tokio::sync::oneshot::Sender<Result<rpc::worker::Response>>,
    )>(20);
    let mut local_processor_stream =
        tokio_stream::wrappers::ReceiverStream::new(local_processor_receiver);
    let mut local_processor_executor = LocalExec::new(local_processor_sender);

    // network requests channel
    let (mut net_leader_sender, net_leader_receiver) = tokio::sync::mpsc::channel::<(
        (SocketAddr, Vec<u8>),
        tokio::sync::oneshot::Sender<Vec<u8>>,
    )>(20);
    let mut net_leader_stream = tokio_stream::wrappers::ReceiverStream::new(net_leader_receiver);
    let net_leader_executor = LocalExec::new(net_leader_sender);

    // local server requests channel
    let (mut local_server_sender, local_server_receiver) = tokio::sync::mpsc::channel::<(
        (Option<ServerId>, rpc::worker::RequestLocal),
        tokio::sync::oneshot::Sender<Result<rpc::worker::Response>>,
    )>(20);
    let mut local_server_stream =
        tokio_stream::wrappers::ReceiverStream::new(local_server_receiver);
    let mut local_server_executor = LocalExec::new(local_server_sender);

    net::spawn_listeners(
        listeners,
        net_leader_executor.clone(),
        runtime.clone(),
        shutdown.clone(),
    )?;

    let clock = watch::channel(0);
    let blocked = tokio::sync::watch::channel(true);
    let mut worker = Worker {
        config,
        id: Uuid::new_v4(),
        leader: None,
        servers: Default::default(),
        blocked_watch: blocked,
        clock_watch: clock,
        part: None,
    };
    let worker = manager::spawn(worker)?;

    // clones to satisfy borrow checker
    let runtime_c = runtime.clone();
    let local_leader_executor_c = local_leader_executor.clone();

    runtime.spawn(async move {
        // let mut streams = StreamMap::new();
        // streams.insert("local_ctl", local_ctl_stream).unwrap();
        // streams.insert("local_server", local_server_stream).unwrap();
        loop {
            let runtime = runtime_c.clone();
            // let (stream, (req, send)) = streams.next().await.unwrap();
            // let resp = handle_local_request(req, worker, runtime).await;
            // send.send(resp);

            tokio::select! {
                Some((req, s)) = local_ctl_stream.next() => {
                    let runtime = runtime.clone();
                    let worker = worker.clone();
                    runtime.clone().spawn(async move {
                        debug!("worker: processing controller message");
                        let resp = handle_local_request(req, worker, runtime.clone()).await;
                        s.send(resp);
                    });
                },
                Some(((server_id, req), s)) = local_server_stream.next() => {
                    let req: rpc::worker::RequestLocal = req;
                    debug!("worker: processing message from local server");

                    let worker = worker.clone();
                    runtime.clone().spawn(async move {
                        // let resp = handle_local_server_request(req, server_id, worker).await;
                        let resp = handle_local_request(req, worker, runtime.clone()).await;
                        s.send(resp);
                    });

                },
                Some((req, s)) = local_leader_stream.next() => {
                    use rpc::worker::{Response, RequestLocal};
                    // worker receives leader executor channel
                    debug!("worker: processing message from local leader");
                    let req: RequestLocal = req;

                    let worker = worker.clone();
                    runtime.clone().spawn(async move {
                        match req {
                            RequestLocal::ConnectLeader(exec) => {
                                log::debug!("worker connecting to leader...");
                                // let mut leader = worker.get_leader().await;
                                // println!(">>> leader: {:?}", leader.is_ok());
                                // let mut leader = leader.unwrap();

                                // leader.exec = LeaderExec::Local(exec);
                                // worker.set_leader(Some(leader)).await;

                                let leader = worker.get_leader().await.map(|mut c| async {
                                    log::debug!("worker already aware of a leader");
                                    c.exec = LeaderExec::Local(exec);
                                    worker.set_leader(Some(c)).await;
                                });
                                log::debug!("worker successfuly connected to leader");

                                // s.send(leader.map(|_| leader_worker::Response::Connect));
                                s.send(Ok(rpc::worker::Response::Connect));

                                // // initiate handshake with leader
                                // let resp: worker_leader::Response = exec.execute(
                                //     (None, worker_leader::RequestLocal::Connect(local_leader_executor_c.clone()))
                                // ).await
                                // .unwrap()
                                // .unwrap();

                                // if let worker_leader::Response::Handshake { worker_id } = resp {
                                //     debug!("leader acknowledged handshake, given id: {}", worker_id);
                                //     worker.leader = Some(Leader {
                                //         exec: LeaderExec::Local(exec),
                                //         worker_id: Some(worker_id),
                                //     });
                                //     s.send(Ok(leader_worker::Response::Handshake(worker_id)));
                                // } else {
                                //     unimplemented!();
                                // }
                            }
                            RequestLocal::Request(req) => {
                                println!(">>> handling local request: {req:?}");
                                let resp = handle_request(req, worker.clone(), runtime.clone()).await;
                                s.send(resp);
                            }
                            _ => todo!()
                        }
                    });

                }
                Some((req, s)) = local_processor_stream.next() => {
                    let worker = worker.clone();
                    runtime.clone().spawn(async move {
                        let resp = handle_request(req, worker, runtime.clone()).await;
                        s.send(resp);
                    });
                },
                Some(((addr, req), s)) = net_leader_stream.next() => {
                    debug!("worker: processing network message from leader");

                    let worker = worker.clone();
                    runtime.clone().spawn(async move {
                        let req: rpc::worker::Request = match decode(&req, Encoding::Bincode) {
                            Ok(r) => r,
                            Err(e) => {
                                error!("failed decoding workplace request (bincode)");
                                return;
                            }
                        };
                        let resp = handle_request(req, worker.clone(), runtime.clone()).await;
                        s.send(encode(resp.map_err(|e| e.to_string()), Encoding::Bincode).unwrap());
                    });
                },
                _ = shutdown.recv() => break,
            }
        }
    });

    Ok(WorkerHandle {
        server_addr: None,
        server_exec: local_server_executor,
        ctl_exec: local_ctl_executor,
        leader_exec: local_leader_executor,
        processor_exec: local_processor_executor,
    })
}

async fn handle_local_request(
    req: rpc::worker::RequestLocal,
    worker: ManagerExec,
    _runtime: runtime::Handle,
) -> Result<rpc::worker::Response> {
    use rpc::worker::RequestLocal;

    match req {
        RequestLocal::ConnectToLeader(_worker_leader, leader_worker) => {
            let mut worker_leader = None;
            let mut resp = _worker_leader
                .execute((
                    None,
                    rpc::leader::RequestLocal::ConnectAndRegisterWorker(leader_worker),
                ))
                .await?;
            let resp = resp
                .and_then(|r| {
                    if let rpc::leader::Response::Register { worker_id } = r {
                        // store the executor so that we can later send requests to the leader
                        worker_leader = Some(Leader {
                            exec: LeaderExec::Local(_worker_leader),
                            worker_id: Some(worker_id),
                        });
                        Ok(rpc::worker::Response::ConnectToLeader { worker_id })
                    } else {
                        Err(Error::UnexpectedResponse("".to_string()))
                    }
                })
                .map_err(|e| Error::FailedConnectingWorkerToLeader(e.to_string()));

            worker.set_leader(worker_leader);

            resp
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
                // worker.lock().await.servers.insert();
                Ok(rpc::worker::Response::ConnectToServer { worker_id })
            } else {
                Err(Error::UnexpectedResponse("".to_string()))
            }
        }
        rpc::worker::RequestLocal::ConnectAndRegisterServer(exec) => {
            let server = Server {
                exec: ServerExec::Local(exec),
                worker_id: None,
            };
            warn!("set the server without worker_id");

            let server_id = Uuid::new_v4();
            worker.insert_server(server_id, server).await?;

            Ok(rpc::worker::Response::Register { server_id })
        }
        RequestLocal::Request(req) => handle_request(req, worker, _runtime).await,
        _ => unimplemented!(),
    }
}

async fn handle_request(
    req: rpc::worker::Request,
    worker: ManagerExec,
    _runtime: runtime::Handle,
) -> Result<rpc::worker::Response> {
    use rpc::worker::{Request, Response};

    trace!("worker: processing controller request");
    match req {
        Request::ConnectToLeader { address } => {
            let bind = SocketAddr::from_str("0.0.0.0:0").unwrap();
            // let endpoint = quinn::Endpoint::client(a).unwrap();
            trace!("worker: connecting to leader: {:?}", address);
            let endpoint = net::quic::make_client_endpoint_insecure(bind).unwrap();
            let connection = endpoint
                .connect(address.address.try_into().unwrap(), "any")
                .unwrap()
                .await
                .unwrap();
            trace!("worker: connected to leader");

            let leader_exec =
                RemoteExec::<rpc::leader::Request, rpc::leader::Response>::new(connection)
                    .await
                    .unwrap();
            trace!("worker: remote executor created");

            let leader_exec = leader_exec.clone();
            // get the data on the leader, e.g. it's Uuid which is provided by the leader itself
            let req = rpc::leader::Request::Register(Uuid::new_v4());
            let resp = leader_exec.execute(req).await.unwrap();
            trace!("worker: sent introduction: got response: {resp:?}");

            if let rpc::leader::Response::Register { worker_id } = resp {
                // store the leader executor so that we can send data to the leader
                // worker.leader = Some(Leader {Remote(leader_exec));
                worker
                    .set_leader(Some(Leader {
                        exec: LeaderExec::Remote(leader_exec),
                        worker_id: Some(worker_id),
                    }))
                    .await?;
            } else {
                unimplemented!("unexpected response: {:?}", resp);
            }

            Ok(Response::Empty)
        }
        Request::Ping(bytes) => Ok(Response::Ping(bytes)),
        Request::MemorySize => {
            let size = worker.memory_size().await?;
            println!("worker mem size: {}", size);
            Ok(Response::MemorySize(size))
        }
        Request::IsBlocking { wait } => {
            let mut is_blocked = worker.get_blocked_watch().await?.clone();
            if *is_blocked.borrow() == true {
                debug!("worker is blocked, waiting to unblock");
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
                debug!("worker is not blocked");
                Ok(Response::IsBlocking(false))
            }
        }
        Request::Step => {
            let current_clock = worker.get_clock_watch().await?.borrow().clone();

            // let (snd, rcv) = tokio::sync::mpsc::channel(32);
            // let exec = LocalExec::new(snd);
            // worker.node_step(exec).await?;

            let new_clock = current_clock + 1;
            worker.set_clock_watch(new_clock).await?;
            for (server_id, server) in worker.get_servers().await? {
                server
                    .execute(rpc::server::Request::ClockChangedTo(new_clock))
                    .await;
            }
            debug!("worker processed step");
            Ok(Response::Step)
        }
        Request::Initialize { model, mod_script } => {
            println!(">>> worker: initializing");

            worker.set_clock_watch(0).await?;

            // initialize part
            let part = Part::from_model(model, mod_script)?;
            worker.set_part(part).await?;

            Ok(Response::Empty)
        }

        Request::Model => {
            println!(">>> worker getting current leader");
            let leader = worker.get_leader().await?;
            println!(">>> worker asking leader for the current model");
            match leader.execute(rpc::leader::Request::Model).await? {
                rpc::leader::Response::Model(model) => Ok(Response::Model(model)),
                _ => panic!(),
            }
        }

        Request::PullModel(model) => {
            // propagate request to leader

            let leader = worker.get_leader().await?;
            let resp = leader
                .execute(rpc::leader::Request::PullModel(model))
                .await?;
            Ok(Response::PullProject)
        }
        Request::Clock => {
            let leader = worker.get_leader().await?;
            debug!("worker_id: {:?}", leader.worker_id);
            let clock = if let rpc::leader::Response::Clock(clock) =
                leader.execute(rpc::leader::Request::Clock).await?
            {
                clock
            } else {
                panic!("worker failed getting clock value from leader");
            };
            Ok(Response::Clock(clock))
        }
        Request::Query(query) => {
            let product = worker.process_query(query).await?;
            // let product = engine_core::query::process_query(
            //     &query,
            //     &worker.lock().await.sim.as_ref().unwrap().entities,
            //     &worker.lock().await.sim.as_ref().unwrap().entity_names,
            // )?;
            debug!("query product: {:?}", product);
            Ok(Response::Query(product))
        }
        Request::SetBlocking(blocking) => {
            worker.set_blocked_watch(blocking).await?;
            debug!("set worker blocked watch to {}", blocking);
            Ok(Response::Empty)
        }
        // Request::Scenario => {
        //     println!(">> worker: start executing request::scenario");
        //     let leader = worker.get_leader().await?;
        //     println!(">> worker: got leader");
        //     if let msg::worker_leader::Response::Scenario(scenario) =
        //         leader.execute(worker_leader::Request::Scenario).await?
        //     {
        //         Ok(Response::Scenario(scenario))
        //     } else {
        //         Err(Error::Other(
        //             "worker failed getting scenario from leader".to_string(),
        //         ))
        //     }
        // }
        Request::EntityList => {
            let resp = worker.execute(manager::Request::GetEntities).await??;
            if let manager::Response::Entities(entities) = resp {
                Ok(Response::EntityList(entities))
            } else {
                Err(Error::UnexpectedResponse(format!(
                    "expected Response::Entities"
                )))
            }
        }

        Request::Query(query) => Ok(Response::Empty),
        _ => unimplemented!("unimplemented: {:?}", req),
    }
}
