use std::borrow::Borrow;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::time::Instant;
use crate::{query, Address, Var};
use fnv::FnvHashMap;

use crate::error::Error;
use crate::portal::SocketEventType::Heartbeat;
use crate::portal::{
    Address as PortalAddress, CompositeAddress, Encoding, Node, NodeConfig, NodeEvent, Transport,
};
use crate::Result;

use super::{ClientConfig, CompressionPolicy};

/// Represents a connection to the server.
///
/// # Blocking client
///
/// A blocking client is one that has to explicitly signal it's ready to
/// proceed to next.
///
/// Blocking is handled on two levels - first on the level of a server, which
/// may have multiple blocking clients connected to it, and second on the level
/// of the leader, which has the ultimate authority when it comes to
/// advancing the simulation clock.
pub struct Client {
    /// Configuration struct
    config: ClientConfig,
    /// Connection to server
    pub connection: Node,
    /// Current connection status
    connected: bool,

    last_poll: Instant,
    time_since_heartbeat: Duration,
}

impl Client {
    pub fn new() -> Result<Self> {
        Self::new_with_config(ClientConfig::default())
    }

    pub fn new_with_config(config: ClientConfig) -> Result<Self> {
        let transport = config
            .transports
            .first()
            .ok_or(Error::Other(
                "client config has to provide at least one transport option".to_string(),
            ))?
            .clone();
        let encoding = config
            .encodings
            .first()
            .ok_or(Error::Other(
                "client config has to provide at least one encoding option".to_string(),
            ))?
            .clone();
        let socket_config = NodeConfig {
            encoding,
            ..Default::default()
        };
        let connection = Node::new_with_config(None, transport, socket_config)?;
        let client = Self {
            config,
            connection,
            connected: false,
            last_poll: Instant::now(),
            time_since_heartbeat: Duration::ZERO,
        };

        Ok(client)
    }

    /// Connects to server at the given address.
    ///
    /// # Redirection
    ///
    /// In it's response to client registration message, the server specifies
    /// a new address at which it started a listener socket. New connection
    /// to that address is then initiated by the client.
    pub fn connect(&mut self, greeter_addr: &str, password: Option<String>) -> Result<()> {
        info!("dialing server greeter at: {}", greeter_addr);

        let greeter_composite: CompositeAddress = greeter_addr.parse()?;

        let mut socket_config = NodeConfig {
            ..Default::default()
        };
        if let Some(_encoding) = greeter_composite.encoding {
            socket_config.encoding = _encoding;
        }
        // let transport = greeter_composite.transport.unwrap_or(Transport::Tcp);
        let transport = greeter_composite
            .transport
            .unwrap_or(self.connection.transport());
        self.connection = Node::new_with_config(None, transport, socket_config)?;

        // self.connection.manual_poll()?;

        self.connection.connect(greeter_composite.address.clone())?;

        // self.connection.manual_poll()?;

        self.connection.send_payload(
            RegisterClientRequest {
                name: self.config.name.clone(),
                is_blocking: self.config.is_blocking,
                auth_pair: None,
                encodings: self.config.encodings.clone(),
                transports: self.config.transports.clone(),
            },
            None,
        )?;
        debug!("sent client registration request");

        // self.connection.manual_poll()?;

        // let mut i: u32 = 0;
        // loop {
        //     i += 1;
        //     if i >= 1000000000 {
        //         break;
        //     }
        // }
        // error!(">>> ended long calculation");

        let resp: RegisterClientResponse = self
            .connection
            .recv_msg()?
            .1
            .unpack_payload(self.connection.encoding())?;
        debug!("got response from server: {:?}", resp);

        // perform redirection using address provided by the server
        if !resp.address.is_empty() {
            self.connection.disconnect(None)?;
            // std::thread::sleep(Duration::from_millis(100));
            // self.connection.disconnect(Some(address))?;
            let composite = CompositeAddress {
                encoding: Some(resp.encoding),
                transport: Some(resp.transport),
                address: resp.address.parse()?,
            };
            if let Some(_encoding) = composite.encoding {
                socket_config.encoding = _encoding;
            }
            if let Some(_transport) = composite.transport {
                self.connection = Node::new_with_config(None, _transport, socket_config)?;
            }
            println!("connecting to: {:?}", composite);
            self.connection.connect(composite.address)?;
            println!("apparent success");
        }

        self.connection.manual_poll()?;

        // if !resp.error.is_empty() {
        //     return Err(Error::Other(resp.error));
        // }

        // self.connection.manual_poll();

        self.connected = true;

        // test msg roundtrip
        // self.connection.send_payload(
        //     StatusRequest {
        //         format: "".to_string(),
        //     },
        //     None,
        // )?;
        // let _ = self.connection.recv_msg()?;

        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        self.connection.disconnect(None)?;
        Ok(())
    }

    pub fn manual_poll(&mut self) -> Result<()> {
        let poll_delta = self.last_poll.elapsed();
        self.time_since_heartbeat += poll_delta;
        if let Some(hb_interval) = self.config.heartbeat_interval {
            if self.time_since_heartbeat >= hb_interval {
                self.connection
                    .send_event(NodeEvent::new(Heartbeat), None)?;
                self.time_since_heartbeat = Duration::ZERO;
                trace!("send heartbeat");
            }
        }
        self.last_poll = Instant::now();
        Ok(())
    }

    pub fn server_status(&mut self) -> Result<StatusResponse> {
        self.connection.send_payload(
            StatusRequest {
                format: "".to_string(),
            },
            None,
        )?;
        // self.connection.manual_poll()?;
        debug!("sent server status request to server");
        let (_, msg) = self.connection.recv_msg()?;
        let resp: StatusResponse = msg.unpack_payload(self.connection.encoding())?;
        Ok(resp)
    }

    pub fn server_step_request(&mut self, steps: u32) -> Result<Message> {
        self.connection.send_payload(
            StepAdvanceRequest {
                step_count: steps,
                wait: true,
            },
            None,
        )?;
        let (_, resp) = self.connection.recv_msg()?;
        Ok(resp)
    }

    // data querying
    pub fn get_var(&mut self, addr: &str) -> Result<Var> {
        // self.connection.send_payload(
        //     DataTransferRequest {
        //         transfer_type: "Select".to_string(),
        //         selection: vec![addr.to_string()],
        //     },
        //     None,
        // )?;
        self.connection.send_payload(
            NativeQueryRequest {
                query: query::Query {
                    trigger: query::Trigger::Immediate,
                    description: query::Description::Addressed,
                    layout: query::Layout::Var,
                    filters: vec![query::Filter::Name(vec![crate::string::new_truncate(
                        "earth_climate",
                    )])],
                    mappings: vec![query::Map::Components(vec![crate::string::new_truncate(
                        "temp",
                    )])],
                },
            },
            None,
        )?;
        let (_, resp) = self.connection.recv_msg()?;
        let mut dtresp: NativeQueryResponse = resp.unpack_payload(self.connection.encoding())?;
        // println!("{:?}", dtresp.query_product);
        let address = Address::from_str(addr)?;
        // println!("desired address: {:?}", address);
        if let query::QueryProduct::AddressedVar(map) = &mut dtresp.query_product {
            map.remove(&address)
                .ok_or(Error::Other("failed getting var".to_string()))
        } else {
            unimplemented!()
        }
    }
    pub fn get_var_as_string(&self, addr: &str) -> Result<String> {
        unimplemented!();
    }
    pub fn get_vars_as_strings(&self, addrs: &Vec<String>) -> Result<Vec<String>> {
        unimplemented!();
    }

    pub fn get_vars(&mut self) -> Result<TransferResponseData> {
        self.connection.send_payload(
            DataTransferRequest {
                transfer_type: "Full".to_string(),
                selection: vec![],
            },
            None,
        )?;
        println!("get_vars sent request");
        let resp: DataTransferResponse = self
            .connection
            .recv_msg()?
            .1
            .unpack_payload(self.connection.encoding())?;

        Ok(resp.data)
    }

    pub fn reg_scheduled_transfer(&mut self) -> Result<()> {
        self.connection.send_payload(
            ScheduledDataTransferRequest {
                event_triggers: vec!["step".to_string()],
                transfer_type: "SelectVarOrdered".to_string(),
                selection: vec!["*:position:float:x".to_string()],
            },
            None,
        )?;
        Ok(())
    }

    // mutation
    pub fn set_vars(&mut self, vars: Vec<(Address, Var)>) -> Result<()> {
        self.connection.send_payload(
            DataPullRequest {
                data: PullRequestData::AddressedVars(vars),
            },
            None,
        )?;
        let resp: DataPullResponse = self
            .connection
            .recv_msg()?
            .1
            .unpack_payload(self.connection.encoding())?;
        if resp.error.is_empty() {
            Ok(())
        } else {
            Err(Error::Other(resp.error))
        }
    }

    // blocking
    pub fn snapshot_request(&mut self, name: String, save_to_disk: bool) -> Result<Vec<u8>> {
        let req = ExportSnapshotRequest {
            name,
            save_to_disk,
            send_back: true,
        };
        self.connection.send_payload(req, None)?;
        println!("sent");
        let (_, v) = self.connection.recv_msg()?;
        println!("received: message type: {:?}", v.type_);
        let resp: ExportSnapshotResponse = v.unpack_payload(self.connection.encoding())?;
        Ok(resp.snapshot)
    }
}
