use futures::TryFutureExt;
use tokio::runtime;
use tokio::sync::oneshot;

use crate::executor::LocalExec;
use crate::rpc;
use crate::Executor;
use crate::Result;

#[derive(Clone)]
pub struct ProcessorHandle {
    triggers: Vec<String>,
    executor: LocalExec<rpc::processor::Request, Result<rpc::processor::Response>>,
}

#[async_trait::async_trait]
impl Executor<rpc::processor::Request, Result<rpc::processor::Response>> for ProcessorHandle {
    async fn execute(
        &self,
        req: rpc::processor::Request,
    ) -> crate::error::Result<Result<rpc::processor::Response>> {
        self.executor.execute(req).await
        // Ok(Ok(rpc::processor::Response::Empty))
    }
}

pub fn spawn<Fut>(
    f: impl FnOnce(
            tokio_stream::wrappers::ReceiverStream<(
                rpc::processor::Request,
                oneshot::Sender<Result<rpc::processor::Response>>,
            )>,
            LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
        ) -> Fut
        + Send
        + 'static,
    processor: LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
    runtime: runtime::Handle,
) -> Result<ProcessorHandle>
where
    Fut: futures::Future<Output = Result<()>> + Send + 'static,
{
    let (mut sender, receiver) = tokio::sync::mpsc::channel::<(
        rpc::processor::Request,
        tokio::sync::oneshot::Sender<Result<rpc::processor::Response>>,
    )>(20);
    let mut stream = tokio_stream::wrappers::ReceiverStream::new(receiver);
    let mut executor = LocalExec::new(sender);

    let r = runtime.spawn(
        f(stream, processor.clone())
            .inspect_err(|e| log::error!("processor failed with: {}", e.to_string())),
    );

    Ok(ProcessorHandle {
        triggers: vec![],
        executor,
    })
}
