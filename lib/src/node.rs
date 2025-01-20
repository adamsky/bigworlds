//! `node` is a runtime wrapper that can instantiate workers, leaders,
//! services within a context of a single process, based on incoming
//! requests.
//!
//! # Callbacks with spawn requests
//!
//! Some constructs may want to call back the node and request spawnining
//! other constructs on the node.
//!
//! Such is the case with workers. When cluster leader is lost, workers need
//! to quickly spawn a new one. They will try to find a suitable node among
//! themselves, requesting node configuration to make sure it supports spawning
//! leaders in the first place.

use std::net::SocketAddr;

use fnv::FnvHashMap;
use tokio::runtime;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;

use crate::executor::{Executor, LocalExec};
use crate::net::{CompositeAddress, ConnectionOrAddress, Encoding, Transport};
use crate::rpc::node::{Request, Response};
use crate::time::Duration;
use crate::util::Shutdown;
use crate::util_net::{decode, encode};
use crate::{leader, net, rpc, server, worker, Error, Relay, Result};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Permissions {
    pub create_workers: bool,
    pub create_leaders: bool,
}

impl Permissions {
    pub fn root() -> Self {
        Self {
            create_workers: true,
            create_leaders: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    // pub threads:
    pub single_threaded: bool,
    pub max_memory: usize,
    pub workers_per_thread: u16,

    pub acl: FnvHashMap<String, Permissions>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        let mut acl = FnvHashMap::default();
        acl.insert("root".to_string(), Permissions::root());
        Self {
            single_threaded: false,
            max_memory: 1000,
            workers_per_thread: 2,
            acl,
        }
    }
}

#[derive(Clone)]
pub struct NodeHandle {
    pub config: NodeConfig,
    pub local_exec: LocalExec<rpc::node::Request, Result<rpc::node::Response>>,
}

pub struct Node {
    pub worker_handles: Vec<worker::Handle>,
}

/// Spawns a `Node` on the provided runtime.
///
/// The returned handle can be used to send requests to the `Node`.
/// Request format is the same as with remote requests coming through the
/// network listener.
pub fn spawn(
    listeners: Vec<CompositeAddress>,
    config: NodeConfig,
    runtime: runtime::Handle,
    mut shutdown: Shutdown,
) -> Result<NodeHandle> {
    let (local_exec, mut local_stream) = LocalExec::new(20);

    // TODO: channel for broadcasting network messages to the main handler.
    // Addressing of responses is based on endpoint address of the incoming
    // message.
    // let (net_sender, net_receiver) = tokio::sync::broadcast::channel::<(SocketAddr, Vec<u8>)>(20);

    let (net_exec, mut net_stream) = LocalExec::new(20);

    net::spawn_listeners(
        listeners,
        net_exec.clone(),
        runtime.clone(),
        shutdown.clone(),
    )?;

    let mut runtime_c = runtime.clone();
    // spawn main handler
    runtime.spawn(async move {
        // processes incoming requests, e.g.:
        // - spawn worker with such and such parameters and connect it to leader
        // - return information on average worker load and number of workers
        // - return some worker in serialized form for migration

        let mut node = Node {
            worker_handles: vec![],
        };

        let mut shutdown_c = shutdown.clone();
        loop {
            tokio::select! {
                Some(((addr, req), s)) = net_stream.next() => {
                    let req = match decode(&req, Encoding::Bincode) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("failed decoding node request (bincode)");
                            continue;
                        }
                    };
                    debug!("node: received net request: {:?}", req);
                    // identify caller and check privileges
                    //

                    let resp = handle_request(req, &mut node, runtime_c.clone(), shutdown.clone());
                    s.send(encode(resp.map_err(|e| e.to_string()), Encoding::Bincode).unwrap());
                },
                Some((req, s)) = local_stream.next() => {
                    debug!("node: received local request: {:?}", req);
                    let resp = handle_request(req, &mut node, runtime_c.clone(), shutdown.clone());
                    s.send(resp);
                },
                _ = shutdown_c.recv() => break,
            };
        }
    });

    // let handlers: Vec<WorkerHandle> = (0..config.workers_per_thread)
    //     .into_iter()
    //     .map(|n| crate::worker::spawn(runtime.clone(), shutdown)?)
    //     .collect();

    Ok(NodeHandle { config, local_exec })
}

fn handle_request(
    req: rpc::node::Request,
    node: &mut Node,
    runtime: runtime::Handle,
    shutdown: Shutdown,
) -> Result<Response> {
    match req {
        Request::Status => Ok(Response::Status {
            worker_count: node.worker_handles.len(),
        }),
        Request::SpawnWorker {
            ram_mb,
            disk_mb,
            server_addr,
            initial_requests,
        } => {
            let mut worker_handle = worker::spawn(
                vec![],
                worker::Config::default(),
                runtime.clone(),
                shutdown.clone(),
            )?;
            let server_exec = worker_handle.server_exec.clone();
            node.worker_handles.push(worker_handle.clone());

            let addr = crate::net::get_available_address()?;

            let listeners = vec![format!("tcp://{}", addr.to_string()).parse()?];
            println!("spawn worker: listeners: {:?}", listeners);

            let server_handle = server::spawn(
                listeners.clone(),
                server::Config::default(),
                worker_handle.clone(),
                runtime.clone(),
                shutdown.clone(),
            )?;

            runtime.clone().spawn(async move {
                for req in initial_requests {
                    trace!("processing initial request: {:?}", req);
                    // unimplemented!();
                    // let resp = worker_handle.ctl_exec.execute(req.clone()).await.unwrap();
                    // trace!("success: resp: {:?}", resp);
                }
            });

            unimplemented!()
            // Ok(rpc::node::Response::SpawnWorker {
            //     listeners,
            //     server_addr: server_handle.listeners.first().cloned(),
            // })
        }
        Request::SpawnLeader { listeners, config } => {
            let leader_handle =
                leader::spawn(listeners.clone(), config, runtime.clone(), shutdown.clone())?;

            Ok(Response::SpawnLeader { listeners })
        }
    }
}
