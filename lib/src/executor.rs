use std::borrow::BorrowMut;
use std::marker::PhantomData;

use quinn::{Endpoint, RecvStream, SendStream};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

#[cfg(feature = "archive")]
use rkyv::de::deserializers::SharedDeserializeMap;
#[cfg(feature = "archive")]
use rkyv::{archived_root, from_bytes_unchecked, Archive};

use crate::{Error, Result};

#[async_trait::async_trait]
pub trait Executor<IN, OUT> {
    async fn execute(&self, msg: IN) -> Result<OUT>;
}

#[derive(Clone)]
pub struct LocalExec<IN, OUT> {
    sender: mpsc::Sender<(IN, oneshot::Sender<OUT>)>,
}

impl<IN, OUT> LocalExec<IN, OUT> {
    pub fn new(
        capacity: usize,
    ) -> (
        Self,
        tokio_stream::wrappers::ReceiverStream<(IN, oneshot::Sender<OUT>)>,
    ) {
        let (mut sender, receiver) =
            tokio::sync::mpsc::channel::<(IN, oneshot::Sender<OUT>)>(capacity);
        let mut stream = tokio_stream::wrappers::ReceiverStream::new(receiver);
        (Self::new_from_sender(sender), stream)
    }

    pub fn new_from_sender(sender: mpsc::Sender<(IN, oneshot::Sender<OUT>)>) -> Self {
        Self { sender }
    }
}

#[async_trait::async_trait]
impl<IN: Send, OUT: Send> Executor<IN, OUT> for LocalExec<IN, OUT> {
    async fn execute(&self, msg: IN) -> Result<OUT> {
        let (sender, receiver) = oneshot::channel::<OUT>();
        self.sender.send((msg, sender)).await.map_err(|e| {
            Error::Other(format!(
                "local executor failed sending, receiver dropped: {e}"
            ))
        })?;
        Ok(receiver.await?)
    }
}

#[derive(Clone)]
pub struct RemoteExec<IN, OUT> {
    connection: quinn::Connection,
    phantom: PhantomData<(IN, OUT)>,
}

impl<IN, OUT> RemoteExec<IN, OUT> {
    pub fn new(connection: quinn::Connection) -> Self {
        RemoteExec {
            connection,
            phantom: Default::default(),
        }
    }

    pub fn remote_address(&self) -> std::net::SocketAddr {
        self.connection.remote_address()
    }
}

#[cfg(feature = "archive")]
#[async_trait::async_trait]
impl<
        IN: Send
            + Sync
            + rkyv::Serialize<
                rkyv::ser::serializers::CompositeSerializer<
                    rkyv::ser::serializers::AlignedSerializer<rkyv::AlignedVec>,
                    rkyv::ser::serializers::FallbackScratch<
                        rkyv::ser::serializers::HeapScratch<1024>,
                        rkyv::ser::serializers::AllocScratch,
                    >,
                    rkyv::ser::serializers::SharedSerializeMap,
                >,
            >,
        OUT: Send + Sync + Archive,
    > Executor<IN, OUT> for RemoteExec<IN, OUT>
where
    OUT::Archived: rkyv::Deserialize<OUT, SharedDeserializeMap>,
{
    async fn execute(&self, msg: IN) -> engine_core::error::Result<OUT> {
        let (mut send, recv) = self.connection.open_bi().await.unwrap();
        trace!("got both ends of stream");
        let msg = rkyv::to_bytes::<_, 1024>(&msg)
            .map_err(|e| engine_core::Error::Other(e.to_string()))?;
        trace!("serialized message");
        send.write_all(&msg).await.unwrap();
        send.finish().await.unwrap();
        trace!("wrote all msg");
        let out_bytes = recv
            .read_to_end(1000000)
            .await
            .map_err(|e| engine_core::Error::Other(e.to_string()))?;
        trace!("outbytes: {:?}", out_bytes);
        let out: OUT = unsafe {
            from_bytes_unchecked(&out_bytes)
                .map_err(|e| engine_core::Error::Other(e.to_string()))?
        };
        Ok(out)
    }
}

#[cfg(not(feature = "archive"))]
#[async_trait::async_trait]
impl<IN: Send + Sync + serde::Serialize, OUT: Send + Sync + serde::de::DeserializeOwned>
    Executor<IN, OUT> for RemoteExec<IN, OUT>
{
    async fn execute(&self, msg: IN) -> Result<OUT> {
        #[cfg(feature = "quic_transport")]
        let out_bytes = {
            let (mut send, recv) = self
                .connection
                .open_bi()
                .await
                .map_err(|e| Error::NetworkError(e.to_string()))?;
            trace!("got both ends of stream");
            let msg = bincode::serialize(&msg).map_err(|e| Error::Other(e.to_string()))?;
            trace!("serialized message");
            send.write_all(&msg)
                .await
                .map_err(|e| Error::NetworkError(e.to_string()));
            send.finish()
                .await
                .map_err(|e| Error::NetworkError(e.to_string()));
            trace!("wrote all msg");
            let out_bytes = recv
                .read_to_end(1000000)
                .await
                .map_err(|e| Error::Other(e.to_string()))?;
            trace!("outbytes: {:?}", out_bytes);
            out_bytes
        };

        #[cfg(not(feature = "quic_transport"))]
        let out_bytes = {};

        let out: OUT = bincode::deserialize(&out_bytes).map_err(|e| Error::Other(e.to_string()))?;
        Ok(out)
    }
}
