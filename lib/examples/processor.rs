//! This example showcases attaching custom machine logic programmatically.

#![allow(warnings)]

use tokio_stream::StreamExt;

use bigworlds::util::Shutdown;
use bigworlds::{machine, rpc, sim, Executor, Query};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut shutdown = Shutdown::new();

    let mut sim = sim::spawn(tokio::runtime::Handle::current(), shutdown.clone()).await?;

    for n in 0..3 {
        let mut shutdown = shutdown.clone();
        let _ = sim
            // Spawn a processor.
            //
            // Processor is code that runs in it's own task on the runtime.
            // It has raw access to the stream of events coming from worker,
            // it can also talk directly to worker using provided executor.
            //
            // Here we spawn a processor from a closure.
            .spawn_processor(|mut stream, worker| async move {

                // state can be persisted during lifetime of the processor
                let state = "Hello, ".to_string();

                tokio::select! {
                    Some((req, s)) = stream.next() => {
                        println!("next");
                        match req {
                            rpc::processor::Request::Trigger(event) => println!("trigger sent: {event}"),
                            rpc::processor::Request::Shutdown => println!("shutdown"),
                        }
                    }
                    _ = shutdown.recv() => {
                        println!("got shutdown signal...");
                        return Ok(());
                    }
                }

                Ok(())

                // let mut value = 0;
                // let mut iterations = 0;
                // loop {
                //     if iterations > 1000 {
                //         // println!("done");
                //         // break;
                //     }
                //     // TODO: only step through on external event called
                //     // tokio::time::sleep(std::time::Duration::from_millis(1)).await;

                //     // TODO: get data from sim
                //     // let data: Vec<(_, u8)> = worker
                //     //     .query(
                //     //         Query::new()
                //     //             .filter(Filter::Component("my_component".to_string()))
                //     //             .map(Map::VarType(engine::VarType::Int))
                //     //             .description(Description::None),
                //     //     )
                //     //     .await
                //     //     .map(|_| ())?;

                //     // println!("print from machine");
                //     value *= 238192873;

                //     use engine::msg::processor_worker::{Request, Response};
                //     let response = worker
                //         .execute(Request::Query(Query::new()))
                //         .await
                //         .unwrap()
                //         .unwrap();

                //     // println!("{response:?}");

                //     iterations += 1;
                // }
            })
            .await;
    }

    sim.invoke("custom_event").await?;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            shutdown.shutdown()?;
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        _ = shutdown.recv() => {
            //
        }
    }

    Ok(())
}
