use crate::time::Duration;
use crate::{Error, Result};

// #[cfg(feature = "async")]
// mod client_async;
// mod client_sync;
// #[cfg(feature = "async")]
// pub use client_async::ClientAsync;
// pub use client_sync::Client;

pub mod r#async;
pub mod sync;

/// Configuration settings for client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Self-assigned name
    pub name: String,
    /// Whether to send regular heartbeats, and with what frequency
    pub heartbeat_interval: Option<Duration>,
    /// Blocking client requires server to wait for it's explicit step advance
    pub is_blocking: bool,
    /// Compression policy for outgoing messages
    pub compress: CompressionPolicy,
    // /// Supported encodings, first is most preferred
    // pub encodings: Vec<Encoding>,
    // /// Supported transports
    // pub transports: Vec<Transport>,
    // /// Should a separate thread be created for polling, or is polling going
    // /// to be performed manually
    // pub polling_thread: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            name: "default_client".to_string(),
            heartbeat_interval: Some(Duration::from_secs(1)),
            is_blocking: false,
            compress: CompressionPolicy::OnlyDataTransfers,
            // encodings: vec![Encoding::Bincode],
            // transports: vec![Transport::Tcp],
            // polling_thread: true,
        }
    }
}

/// List of available compression policies for outgoing messages.
#[derive(Debug, Clone)]
pub enum CompressionPolicy {
    /// Compress all outgoing traffic
    Everything,
    /// Only compress messages larger than given size in bytes
    LargerThan(usize),
    /// Only compress data-heavy messages
    OnlyDataTransfers,
    /// Don't use compression
    Nothing,
}

impl CompressionPolicy {
    pub fn from_str(s: &str) -> Result<Self> {
        if s.starts_with("bigger_than_") || s.starts_with("larger_than_") {
            let split = s.split('_').collect::<Vec<&str>>();
            let number = split[2]
                .parse::<usize>()
                .map_err(|e| Error::Other(e.to_string()))?;
            return Ok(Self::LargerThan(number));
        }
        let c = match s {
            "all" | "everything" => Self::Everything,
            "data" | "only_data" => Self::OnlyDataTransfers,
            "none" | "nothing" => Self::Nothing,
            _ => {
                return Err(Error::Other(format!(
                    "failed parsing compression policy from string: {}",
                    s
                )))
            }
        };
        Ok(c)
    }
}
