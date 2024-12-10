use std::net::SocketAddr;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::runtime;
use tokio::sync::oneshot;
use tokio_tungstenite;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

use crate::executor::{Executor, LocalExec};
use crate::util::Shutdown;
use crate::{Error, Result};

pub fn spawn_listener(
    address: SocketAddr,
    exec: LocalExec<(SocketAddr, Vec<u8>), Vec<u8>>,
    runtime: runtime::Handle,
    mut shutdown: Shutdown,
) {
    let runtime_c = runtime.clone();
    runtime.spawn(async move {
        let listener = TcpListener::bind(address).await.unwrap();
        loop {
            let (mut stream, addr) = listener.accept().await.unwrap();
            let mut ws = match tokio_tungstenite::accept_async(stream).await {
                Ok(w) => w,
                Err(e) => {
                    error!("websocket handshake failed: {:?}", e);
                    continue;
                }
            };
            let (mut write, mut read) = ws.split();

            let (snd, mut rcv) = tokio::sync::mpsc::channel::<Vec<u8>>(20);
            let mut rcv_stream = tokio_stream::wrappers::ReceiverStream::new(rcv)
                .map(|b| Ok(tokio_tungstenite::tungstenite::Message::Binary(b)));
            runtime_c.spawn(async move {
                write.send_all(&mut rcv_stream).await;
            });

            let exec_c = exec.clone();
            let mut shutdown_c = shutdown.clone();
            let runtime_cc = runtime_c.clone();
            runtime_c.spawn(async move {
                loop {
                    // println!("reading");
                    let mut exec_cc = exec_c.clone();
                    let snd_c = snd.clone();
                    tokio::select! {
                        Some(Ok(msg)) = read.next() => {
                            let addr_c = addr.clone();
                            runtime_cc.spawn(async move {
                                let msg: Message = msg;
                                let bytes = match msg {
                                    Message::Binary(b) => b,
                                    Message::Close(c) => {
                                        if let Some(c_) = c {
                                            warn!("close frame: {:?}", c_);
                                        }
                                        return;
                                    },
                                    _ => unimplemented!("{:?}", msg),
                                };

                                match exec_cc.execute((addr_c, bytes)).await {
                                    Ok(resp_bytes) => if let Err(e) = snd_c.send(resp_bytes).await {
                                        error!("{}", e.to_string());
                                    },
                                    Err(e) => error!("{}", e.to_string()),
                                };

                            });
                        }
                        _ = shutdown_c.recv() => break,
                    }
                }
            });
        }
    });
}
