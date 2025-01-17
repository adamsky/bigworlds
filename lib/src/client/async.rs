use async_trait::async_trait;
use tokio::runtime;

use crate::client::ClientConfig;
use crate::net::CompositeAddress;
use crate::query::{Query, QueryProduct};
use crate::rpc::msg::{Message, StatusResponse};
use crate::util::Shutdown;
use crate::{Address, Result};

#[async_trait]
pub trait AsyncClient {
    type Client;

    async fn connect(
        addr: CompositeAddress,
        config: ClientConfig,
        runtime: runtime::Handle,
        shutdown: Shutdown,
    ) -> Result<Self::Client>;
    async fn disconnect(&mut self) -> Result<()>;

    async fn status(&mut self) -> Result<StatusResponse>;
    async fn step_request(&mut self, step_count: u32) -> Result<()>;
    async fn query(&mut self, q: Query) -> Result<QueryProduct>;

    fn send_msg(&mut self, msg: &Message) -> Result<()>;
    async fn recv_msg(&mut self) -> Result<Message>;
    fn try_recv_msg(&mut self) -> Result<Message>;
}
