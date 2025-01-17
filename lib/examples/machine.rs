//! This example showcases executing custom `machine` logic programmatically.
//!
//! `machine` is a special kind of `behavior`. It allows executing a set of
//! predefined commands within a simple state machine regime.

#![allow(warnings)]

use tokio_stream::StreamExt;

use bigworlds::util::Shutdown;
use bigworlds::{machine, rpc, sim, Executor, Query};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut shutdown = Shutdown::new();

    // spawn a local simulation instance
    let mut sim = sim::spawn(shutdown.clone()).await?;

    // spawn an empty machine
    let mut machine = sim.spawn_machine().await?;

    // force-execute an arbitrary command on the machine
    let response = machine.execute_cmd(machine::Command::NoOp).await?;
    assert_eq!(response, rpc::machine::Response::Empty);

    // shut the machine down
    machine.shutdown().await?;

    // this shouldn't run as we already sent the shutdown signal
    let result = machine.execute_cmd(machine::Command::NoOp).await;
    assert!(result.is_err());

    Ok(())
}
