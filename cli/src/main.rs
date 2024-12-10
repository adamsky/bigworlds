//! Command line program for working with `bigworlds` simulations.

#![allow(unused)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod cli;
mod client;
mod interactive;
mod node;
mod run;
mod server;
mod tracing;
mod util;

use colored::*;

fn main() {
    // let runtime_threads = std::env::var("RUNTIME_THREADS").unwrap_or("4".to_string());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        // .worker_threads(runtime_threads.parse().unwrap())
        // .worker_threads(4)
        // let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed building tokio runtime");
    let runtime_handle = runtime.handle().clone();

    runtime.block_on(async move {
        // Run the program based on user input
        match cli::start(cli::arg_matches(), runtime_handle).await {
            Ok(_) => (),
            Err(e) => {
                print!("{}{}\n", "error: ".red(), e);
                if e.root_cause().to_string() != e.to_string() {
                    println!("Caused by:\n{}", e.root_cause())
                }
            }
        }
    });
}
