use futures::future::BoxFuture;
use futures::TryFutureExt;
use tokio::runtime;
use tokio::sync::oneshot;

use crate::executor::LocalExec;
use crate::rpc;
use crate::Executor;
use crate::Result;

#[derive(Clone)]
pub struct BehaviorHandle {
    triggers: Vec<String>,
    executor: LocalExec<rpc::behavior::Request, Result<rpc::behavior::Response>>,
}

#[async_trait::async_trait]
impl Executor<rpc::behavior::Request, Result<rpc::behavior::Response>> for BehaviorHandle {
    async fn execute(
        &self,
        req: rpc::behavior::Request,
    ) -> crate::error::Result<Result<rpc::behavior::Response>> {
        self.executor.execute(req).await
    }
}

/// Spawns a new unsynced behavior task on the runtime.
pub fn spawn(
    f: impl FnOnce(
        tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
        LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
    ) -> BoxFuture<'static, Result<()>>,
    broadcast: tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
    exec: LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
    runtime: runtime::Handle,
) -> Result<()> {
    let r = runtime.spawn(
        f(broadcast, exec.clone())
            .inspect_err(|e| log::error!("behavior failed with: {}", e.to_string())),
    );

    Ok(())
}

pub fn spawn_synced(
    f: impl FnOnce(
        tokio_stream::wrappers::ReceiverStream<(
            rpc::behavior::Request,
            oneshot::Sender<Result<rpc::behavior::Response>>,
        )>,
        LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
    ) -> BoxFuture<'static, Result<()>>,
    exec: LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
    runtime: runtime::Handle,
) -> Result<BehaviorHandle> {
    let (executor, stream) = LocalExec::new(20);

    let r = runtime.spawn(
        f(stream, exec.clone())
            .inspect_err(|e| log::error!("behavior failed with: {}", e.to_string())),
    );

    Ok(BehaviorHandle {
        triggers: vec![],
        executor,
    })
}

pub type BehaviorFnUnsynced = fn(
    tokio::sync::broadcast::Receiver<rpc::behavior::Request>,
    LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
) -> BoxFuture<'static, Result<()>>;
