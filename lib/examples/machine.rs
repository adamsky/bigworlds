//! This example showcases attaching custom machine logic programmatically.

#![allow(warnings)]

use tokio_stream::StreamExt;

use bigworlds::util::Shutdown;
use bigworlds::{machine, rpc, sim, Executor, Query};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut shutdown = Shutdown::new();

    let mut sim = sim::spawn(tokio::runtime::Handle::current(), shutdown.clone()).await?;

    let mut machine = sim.spawn_machine().await?;
    let response = machine.execute_cmd(machine::Command::NoOp).await?;
    println!("cmd response: {response:?}");
    machine.shutdown().await?;
    println!("machine shutdown complete");

    // this shouldn't run as we already sent the shutdown signal
    let response = machine.execute_cmd(machine::Command::NoOp).await?;
    println!("{response:?}");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            shutdown.shutdown()?;
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        _ = shutdown.recv() => {
            println!("shutting down");
        }
    }

    Ok(())
}
