use std::net::SocketAddr;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use tokio::sync::oneshot::Sender;
use tokio_stream::wrappers::ReceiverStream;

use engine_core::common::util::Shutdown;
use engine_core::executor::Executor;

use crate::exec::LocalExec;
use crate::msg::client_server::StatusRequest;
use crate::msg::Message;
use crate::Result;

pub struct HttpServerHandler {}

pub fn spawn(
    interface: LocalExec<Message, Result<Message>>,
    mut shutdown: Shutdown,
) -> HttpServerHandler {
    let handle = tokio::spawn(async move {
        let router = Router::new()
            .route("/status", get(status))
            .layer(Extension(interface));

        // run our app with hyper
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        log::debug!("listening on {}", addr);
        tokio::select! {
            _ = axum::Server::bind(&addr)
                .serve(router.into_make_service()) => (),
            _ = shutdown.recv() => (),
        };
    });

    HttpServerHandler {}
}

async fn status(
    Extension(mut interface): Extension<LocalExec<Message, Message>>,
) -> impl IntoResponse {
    println!("handling status");
    let result = interface
        .execute(Message::StatusRequest(StatusRequest {
            format: "".to_string(),
        }))
        .await
        .unwrap();
    Json(result)
}
