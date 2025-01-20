use std::path::PathBuf;
use std::str::FromStr;

use futures::future::BoxFuture;
use tokio::runtime;
use uuid::Uuid;

use crate::entity::Entity;
use crate::executor::{Executor, LocalExec};
use crate::rpc::msg::{AdvanceRequest, Message, RegisterClientRequest, RegisterClientResponse};
use crate::rpc::worker::RequestLocal;
use crate::util::Shutdown;
use crate::{behavior, string};
use crate::{
    leader, rpc, server, worker, Address, EntityId, EntityName, Error, Model, Result, Var,
};

#[cfg(feature = "machine")]
use crate::machine::MachineHandle;

/// Spawns a local simulation instance.
///
/// This is a convenient method for setting up a simulation instance with
/// a simple interface.
///
/// Such spawned simulation still needs to be initialized with a model. See
/// `SimHandle::initialize` or `SimHandle::spawn_from`.
pub async fn spawn(shutdown: Shutdown) -> Result<SimHandle> {
    spawn_on(tokio::runtime::Handle::current(), shutdown).await
}

/// Spawns a local simulation instance on the provided runtime.
pub async fn spawn_on(runtime: runtime::Handle, shutdown: Shutdown) -> Result<SimHandle> {
    // spawn leader task
    let mut leader_handle = leader::spawn(
        vec![],
        leader::Config {
            // TODO: don't do autostep before initializing the cluster
            autostep: Some(std::time::Duration::from_micros(10)),
            ..Default::default()
        },
        runtime.clone(),
        shutdown.clone(),
    )?;

    // spawn worker task
    let worker_handle = worker::spawn(
        vec![],
        worker::Config::default(),
        runtime.clone(),
        shutdown.clone(),
    )?;

    // make sure leader and worker can talk to each other
    leader_handle
        .connect_to_worker(&worker_handle, true)
        .await?;

    // spawn server task
    let mut server_handle = server::spawn(
        // client listeners will be set up on the following addresses
        vec![
            "tcp://127.0.0.1:9123".parse()?,
            #[cfg(feature = "ws_transport")]
            "ws://127.0.0.1:9223".parse()?,
        ],
        server::Config::default(),
        worker_handle.clone(),
        runtime,
        shutdown,
    )?;

    // attach the server to the worker
    server_handle
        .connect_to_worker(&worker_handle, true)
        .await?;

    // register as a new client connecting to the server
    // TODO: hide this code within server handle implementation
    let resp = server_handle
        .execute(Message::RegisterClientRequest(RegisterClientRequest {
            name: "sim_handle".to_string(),
            is_blocking: true,
            auth_pair: None,
            encodings: vec![],
            transports: vec![],
        }))
        .await??;

    // save the returned client id for use when querying the server
    let client_id = if let Message::RegisterClientResponse(RegisterClientResponse {
        client_id,
        encoding,
        transport,
        redirect_to,
    }) = resp
    {
        Uuid::from_str(&client_id).unwrap()
    } else {
        unimplemented!()
    };
    server_handle.client_id = Some(client_id);

    let handle = SimHandle {
        server: server_handle,
        leader: leader_handle,
        worker: worker_handle,
    };
    Ok(handle)
}

/// Convenience function for spawning simulation from provided model and
/// applying scenario selected by name.
pub async fn spawn_from(
    model: Model,
    scenario: Option<&str>,
    runtime: runtime::Handle,
    shutdown: Shutdown,
) -> Result<SimHandle> {
    // Spawn a raw simulation instance.
    let sim_handle = spawn_on(runtime, shutdown).await?;

    // Initialize the simulation using provided model.
    sim_handle.pull_model(model).await?;
    sim_handle
        .initialize(scenario.map(|s| s.to_owned()))
        .await?;

    Ok(sim_handle)
}

/// Convenience function for spawning simulation from provided path to model
/// and optionally applying scenario selected by name.
pub async fn spawn_from_path(
    model_path: PathBuf,
    scenario: Option<&str>,
    runtime: runtime::Handle,
    shutdown: Shutdown,
) -> Result<SimHandle> {
    // create the model from path
    // TODO: move path handling business, perhaps to `Model::from_path`
    let current_path = std::env::current_dir().expect("failed getting current dir path");
    let path_buf = PathBuf::from(model_path);
    let path_to_model = current_path.join(path_buf);
    let model = Model::from_files(&vfs::PhysicalFS::new(path_to_model), None)?;

    let runtime = tokio::runtime::Handle::current();
    spawn_from(model, scenario, runtime, shutdown).await
}

/// Local simulation instance handle.
pub struct SimHandle {
    pub server: server::Handle,
    pub leader: leader::Handle,
    pub worker: worker::Handle,
}

impl SimHandle {
    /// Registers new machine for instancing based on requirements.
    // TODO: perhaps provide machine instructions as argument
    #[cfg(feature = "machine")]
    pub async fn register_machine(
        &mut self,
        reqs: Vec<crate::machine::SpawnRequirement>,
    ) -> Result<MachineHandle> {
        // self.worker
        unimplemented!()
    }

    /// Spawns new machine task.
    #[cfg(feature = "machine")]
    pub async fn spawn_machine(&mut self) -> Result<MachineHandle> {
        use crate::machine;

        let runtime = tokio::runtime::Handle::current();
        let machine = machine::spawn(self.worker.behavior_exec.clone(), runtime)?;
        Ok(machine)
    }

    /// Spawns new behavior task based on the provided closure.
    ///
    /// TODO: take in an optional collection of triggers. `None` would mean
    /// continuous behavior without explicit external triggering.
    pub async fn spawn_behavior(
        &mut self,
        f: impl FnOnce(
            tokio_stream::wrappers::ReceiverStream<(
                rpc::behavior::Request,
                tokio::sync::oneshot::Sender<Result<rpc::behavior::Response>>,
            )>,
            LocalExec<rpc::worker::Request, Result<rpc::worker::Response>>,
        ) -> BoxFuture<'static, Result<()>>,
    ) -> Result<behavior::BehaviorHandle> {
        let runtime = tokio::runtime::Handle::current();
        behavior::spawn_synced(f, self.worker.behavior_exec.clone(), runtime)
    }

    /// Broadcasts an event accross the simulation.
    pub async fn invoke(&mut self, event: &str) -> Result<()> {
        self.worker
            .ctl
            .execute(rpc::worker::Request::Trigger(string::new_truncate(event)).into())
            .await??;
        Ok(())
    }

    /// Steps the simulation forward.
    ///
    /// # Optional nature of synchronization
    ///
    /// Stepping the simulation simply means emitting a simulation-wide `step`
    /// event and incrementing the internal clock.
    ///
    /// Synchronization is not required however. It's possible that none of the
    /// `behavior`s, `server`s, etc. that are part of the current simulation
    /// setup will choose to observe and/or act upon `step` events.
    ///
    /// It's also possible that it will be a mixed bag, some parts of the
    /// system can make use of synchronization and others can remain
    /// "real-time".
    pub async fn step(&mut self) -> Result<()> {
        use crate::rpc::server::Request as ServerRequest;

        // let now = std::time::Instant::now();

        let resp = self
            .server
            .execute(Message::AdvanceRequest(AdvanceRequest {
                step_count: 1,
                wait: true,
            }))
            .await??;

        // println!("stepped in {}ms", now.elapsed().as_millis());

        Ok(())
    }

    pub async fn entities(&mut self) -> Result<Vec<EntityName>> {
        let resp = self.server.execute(Message::EntityListRequest).await??;
        match resp {
            Message::EntityListResponse(entities) => Ok(entities),
            _ => Err(Error::UnexpectedResponse(format!(
                "expected EntityListResponse, got {:?}",
                resp
            ))),
        }
    }

    pub async fn pull_model(&self, model: Model) -> Result<()> {
        use rpc::leader::{Request, Response};
        self.leader
            .ctl
            .execute(Request::PullModel(model).into())
            .await?
            .map(|_| ())
    }

    /// Initializes simulation state using loaded model.
    ///
    /// Initialization process can optionally take a scenario, which is
    /// effectively a set of additional initialization rules.
    pub async fn initialize(&self, scenario: Option<String>) -> Result<()> {
        use rpc::leader::{Request, Response};

        self.leader
            .ctl
            .execute(Request::Initialize { scenario }.into())
            .await?
            .map(|_| ())
    }

    /// Initializes cluster with a scenario
    pub async fn load_scenario(&self, path: String) -> Result<()> {
        unimplemented!()
    }

    pub async fn load_snapshot(&self, path: String) -> Result<()> {
        unimplemented!()
    }

    pub async fn spawn_entity(&self, name: String, prefab: String) -> Result<()> {
        let resp = self
            .leader
            .ctl
            .execute(rpc::leader::Request::SpawnEntity { name, prefab }.into())
            .await??;
        match resp {
            rpc::leader::Response::Empty => Ok(()),
            _ => Err(Error::UnexpectedResponse(format!("{resp}"))),
        }
    }

    pub async fn model(&mut self) -> Result<Model> {
        Ok(self
            .worker
            .ctl
            .execute(rpc::worker::Request::GetModel.into())
            .await??
            .try_into()?)
    }

    pub async fn query(&self, query: crate::Query) -> Result<crate::QueryProduct> {
        Ok(self
            .worker
            .ctl
            .execute(rpc::worker::Request::Query(query).into())
            .await??
            .try_into()?)
    }

    pub async fn get_var(&self, addr: Address) -> Result<Var> {
        Ok(self
            .worker
            .ctl
            .execute(rpc::worker::Request::GetVar(addr).into())
            .await??
            .try_into()?)
    }

    pub async fn set_var(&self, addr: Address, var: Var) -> Result<()> {
        self.worker
            .ctl
            .execute(rpc::worker::Request::SetVar(addr, var).into())
            .await??;
        Ok(())
    }

    pub async fn get_clock(&self) -> Result<usize> {
        let resp = self.leader.execute(rpc::leader::Request::Clock).await??;
        if let rpc::leader::Response::Clock(clock) = resp {
            Ok(clock)
        } else {
            unimplemented!()
        }
    }

    /// Initiates proper shutdown by propagating the proper signal accross all
    /// running tasks.
    pub async fn shutdown(&self) -> Result<()> {
        trace!("initiating shutdown on local sim instance");
        self.worker.shutdown().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::rpc::msg::Message;
    use crate::util::Shutdown;
    use crate::{sim, Executor, Result};

    #[tokio::test]
    async fn server_ping() -> Result<()> {
        let handle = sim::spawn(Shutdown::new()).await?;

        let response = handle
            .server
            .execute(Message::PingRequest(vec![1; 5]))
            .await??;

        assert_eq!(response, Message::PingResponse(vec![1; 5]));

        Ok(())
    }
}
