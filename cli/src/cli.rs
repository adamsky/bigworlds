//! Application definition.

use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{env, thread};

use anyhow::{Error, Result};
use bytes::BufMut;
use clap::builder::PossibleValue;
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};
use directories::ProjectDirs;
use fnv::FnvHashMap;
use reqwest::header::AUTHORIZATION;
use reqwest::StatusCode;
use tokio::runtime;
use uuid::Uuid;

use notify::{RecommendedWatcher, Watcher};

use bigworlds::leader::LeaderConfig;
use bigworlds::net;
use bigworlds::net::{CompositeAddress, Transport};
use bigworlds::util::get_snapshot_paths;
use bigworlds::util::Shutdown;
use bigworlds::worker::WorkerConfig;
use bigworlds::SimHandle;
use bigworlds::{leader, server, worker, ServerConfig};
use bigworlds::{rpc, Executor};

use crate::interactive::{OnShutdown, OnShutdownAction};
use crate::tracing::LogLevel;
use crate::util::format_elements_list;
use crate::{client, interactive};

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const AUTHORS: &'static str = env!("CARGO_PKG_AUTHORS");

pub fn arg_matches() -> ArgMatches {
    let cmd = Command::new("bigworlds-cli")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .version(VERSION)
        .author(AUTHORS)
        .about("Simulate big worlds from the command line.")
        .arg(
            Arg::new("verbosity")
                .long("verbosity")
                .short('v')
                .display_order(100)
                .value_name("level")
                .default_value("info")
                .value_parser(["trace", "debug", "info", "warn", "error", "none"])
                .global(true)
                .help("Set the verbosity of the log output"),
        )
        .subcommand(
            Command::new("run")
                .about("Run simulation locally")
                .display_order(20)
                .long_about(
                    "Run simulation from scenario, snapshot or experiment.\n\
                If there are no arguments supplied the program will look for a scenario,\n\
                snapshot or experiment (in that order) in the current working directory.",
                )
                .arg(Arg::new("path").value_name("path"))
                .arg(
                    Arg::new("scenario")
                        .long("scenario")
                        .short('s')
                        .num_args(0..)
                        .default_missing_value("__any")
                        .help("Starts new world using a scenario manifest file"),
                )
                .arg(
                    Arg::new("snapshot")
                        .long("snapshot")
                        .short('n')
                        .help("Starts new world using a snapshot file"),
                )
                .arg(
                    Arg::new("server")
                        .long("server")
                        .help("Expose a server, allowing for attaching clients and services")
                        .value_name("SERVER_ADDRESS")
                        .default_value("127.0.0.1:0"),
                )
                .arg(
                    Arg::new("interactive")
                        .action(ArgAction::SetTrue)
                        .long("interactive")
                        .short('i')
                        .default_value("true"),
                )
                .arg(
                    Arg::new("icfg")
                        .long("icfg")
                        .help("Specify path to interactive mode configuration file")
                        .value_name("path")
                        .default_value("./interactive.yaml"),
                )
                .arg(
                    Arg::new("watch")
                        .long("watch")
                        .help("Watch project directory for changes")
                        .value_name("on-change")
                        .value_parser([
                            PossibleValue::new("restart"),
                            PossibleValue::new("update"),
                        ]),
                ),
        )
        // client
        .subcommand(client::cmd());

    // server
    let cmd = cmd.subcommand(crate::server::cmd());

    let cmd = cmd.subcommand(
        Command::new("worker")
            .about("Start a worker")
            .long_about(
                "Start a worker. Worker is the smallest independent part\n\
            of a system where a collection of worker nodes collaboratively\n\
            simulate a larger world.\n\n\
            Worker must have a connection to the main leader, whether direct\n\
            or indirect. Indirect connection to leader can happen through another\n\
            worker or a relay.",
            )
            .display_order(23)
            .arg(
                Arg::new("address")
                    .long("address")
                    .short('a')
                    .help("Set the address for the worker")
                    .value_name("address"),
            )
            .arg(
                Arg::new("leader")
                    .long("leader")
                    .short('o')
                    .help("Address of the cluster leader to connect to")
                    .value_name("address"),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("Establish a server backed by this worker")
                    .num_args(0..)
                    .value_name("address"),
            )
            .arg(
                Arg::new("max_ram")
                    .long("max_ram")
                    .help("Maximum allowed memory usage")
                    .value_name("megabytes"),
            )
            .arg(
                Arg::new("max_disk")
                    .long("max_disk")
                    .help("Maximum allowed disk usage")
                    .value_name("megabytes"),
            )
            .arg(
                Arg::new("max_transfer")
                    .long("max_transfer")
                    .help("Maximum allowed network transfer usage")
                    .value_name("megabytes"),
            ),
    );

    let cmd = cmd.subcommand(
        Command::new("node")
            .about("Start a node")
            .display_order(24)
            .arg(
                Arg::new("config")
                    .long("config")
                    .help("Path to configuration file")
                    .value_name("path"),
            )
            .arg(
                Arg::new("listeners")
                    .long("listeners")
                    .short('l')
                    .help("List of listener addresses")
                    .num_args(1..)
                    .value_name("address"),
            )
            .arg(
                Arg::new("leader")
                    .long("leader")
                    .short('c')
                    .help("Address of the cluster leader to contact")
                    .value_name("address"),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .help("Establish a server at the level of the workplace")
                    .num_args(1..)
                    .value_name("address"),
            ),
    );

    let cmd = cmd.subcommand(
        Command::new("leader")
            .about("Start a cluster leader")
            .display_order(26)
            .arg(
                Arg::new("listeners")
                    .long("listeners")
                    .short('l')
                    .help("List of listener addresses, delineated with a coma ','")
                    .value_name("addresses"),
            )
            .arg(
                Arg::new("workers")
                    .long("workers")
                    .short('w')
                    .help("Addresses of the workers to connect to, delineated with a coma ','")
                    .value_name("addresses"),
            ),
    );

    cmd.get_matches()
}

/// Runs based on specified subcommand.
pub async fn start(matches: ArgMatches, runtime: runtime::Handle) -> Result<()> {
    init_logging(&matches);

    // set up mechanism for graceful shutdown
    let mut shutdown = Shutdown::new();

    match matches.subcommand() {
        Some(("run", m)) => crate::run::start(m, shutdown.clone()).await?,
        Some(("client", m)) => crate::client::start(m, runtime, shutdown.clone()).await?,
        Some(("worker", m)) => start_worker(m, runtime, shutdown.clone()).await?,
        Some(("leader", m)) => start_leader(m, runtime, shutdown.clone()).await?,
        Some(("node", m)) => crate::node::start(m, runtime, shutdown.clone()).await?,
        _ => (),
    }

    // Wait for either ctrl_c signal or message from within server task(s)
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Initiating graceful shutdown...");
            // shutdown_send.send(()).map(|_| ())?;
            shutdown.shutdown()?;
        },
        _ = shutdown.recv() => {},
    }

    tokio::time::sleep(Duration::from_millis(300)).await;

    Ok(())
}
async fn start_worker(
    matches: &ArgMatches,
    runtime: runtime::Handle,
    shutdown: Shutdown,
) -> Result<()> {
    // extract the addresses to listen on
    let listeners = matches
        .get_one::<String>("address")
        .map(|s| s.split(',').collect::<Vec<&str>>())
        .map(|v| {
            v.iter()
                .filter_map(|s| s.parse::<CompositeAddress>().ok())
                .collect()
        })
        .unwrap_or(vec![CompositeAddress::available_net()?]);

    // create worker configuration
    let mut config = WorkerConfig::default();
    config.addr = matches.get_one::<String>("address").cloned();
    config.max_ram_mb = matches.get_one::<usize>("max_ram").unwrap_or(&0).to_owned();
    config.max_disk_mb = matches
        .get_one::<usize>("max_disk")
        .unwrap_or(&0)
        .to_owned();
    config.max_transfer_mb = matches
        .get_one::<usize>("max_transfer")
        .unwrap_or(&0)
        .to_owned();

    // spawn worker task, returning a handle with executors
    let mut worker = worker::spawn(listeners, config, runtime.clone(), shutdown.clone())?;

    // if leader address is specified, attempt a connection
    if let Some(leader_addr) = matches.get_one::<String>("leader") {
        let leader_addr = leader_addr.parse()?;
        if let Err(e) = worker
            .ctl_exec
            .execute(
                bigworlds::rpc::worker::Request::ConnectToLeader {
                    address: leader_addr,
                }
                .into(),
            )
            .await
        {
            error!("{}", e.to_string());
        }
    }

    Ok(())
}

/// Starts a leader task on the provided runtime.
async fn start_leader(
    matches: &ArgMatches,
    runtime: runtime::Handle,
    shutdown: Shutdown,
) -> Result<()> {
    let listeners = matches
        .get_one::<String>("listeners")
        .map(|s| s.split(',').collect::<Vec<&str>>())
        .map(|v| {
            v.iter()
                .filter_map(|s| s.parse::<CompositeAddress>().ok())
                .collect()
        })
        .unwrap_or(vec![CompositeAddress::available_net()?]);
    let config = LeaderConfig::default();

    let handle = leader::spawn(listeners, config, runtime.clone(), shutdown.clone())?;

    Ok(())
}

/// Sets up logging based on settings from the matches.
fn init_logging(matches: &ArgMatches) -> Result<()> {
    let log_level = match matches.get_one::<String>("verbosity") {
        Some(s) => match s.as_str() {
            "0" | "none" => Some(LogLevel::Off),
            "1" | "err" | "error" | "min" => Some(LogLevel::Error),
            "2" | "warn" | "warning" | "default" => Some(LogLevel::Warn),
            "3" | "info" => Some(LogLevel::Info),
            "4" | "debug" => Some(LogLevel::Debug),
            "5" | "trace" | "max" | "all" => Some(LogLevel::Trace),
            _ => None,
        },
        _ => None,
    };
    crate::tracing::init(
        format!(
            "bigworlds-cli@{}",
            hostname::get().unwrap().to_string_lossy()
        ),
        log_level,
    )
}
