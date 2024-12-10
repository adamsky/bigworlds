use crate::client::ClientConfig;
use crate::query::{Query, QueryProduct};
use crate::{Address, Result};

pub trait SyncClient {
    type Client;
    fn new(config: ClientConfig) -> Result<Self::Client>;
    fn connect(addr: &str) -> Result<()>;
    fn disconnect() -> Result<()>;

    fn status() -> Result<String>;
    fn query(q: Query) -> Result<QueryProduct>;
}
