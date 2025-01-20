use anyhow::{Error, Result};
use clap::ArgMatches;
use tokio::runtime;

use bigworlds::{net::CompositeAddress, server, util::Shutdown, worker, Executor};

pub fn cmd() -> clap::Command {
    use clap::{builder::PossibleValue, Arg, Command};

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
                .help("Set the listener address(es) for the worker")
                .value_name("address"),
        )
        .arg(
            Arg::new("remote-worker")
                .long("remote-worker")
                .help("Address of a remote worker to connect to")
                .value_name("address"),
        )
        .arg(
            Arg::new("remote-leader")
                .long("remote-leader")
                .help("Address of the cluster leader to connect to")
                .value_name("address"),
        )
        .arg(
            Arg::new("leader")
                .long("leader")
                .short('l')
                .help("Establish a leader on the same process")
                .num_args(0..)
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
        )
}

pub async fn start(
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
    let mut config = worker::Config::default();
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

    // if server flag is present spawn a server task
    if let Some(listeners) = matches.get_many::<String>("server") {
        println!("spawning server");
        let server = server::spawn(
            listeners
                .into_iter()
                .filter_map(|l| l.parse().ok())
                .collect::<Vec<_>>(),
            server::Config::default(),
            worker.clone(),
            runtime.clone(),
            shutdown.clone(),
        )?;
        worker.connect_to_local_server(&server).await?;
    }

    // if worker addresses are specified, attempt to connect
    if let Some(worker_addrs) = matches.get_many::<String>("worker") {
        for addr in worker_addrs {
            // let addr = addr.parse()?;
            if let Err(e) = worker.connect_to_worker(addr).await {
                error!("{e}");
            }
        }
    }

    // if leader address is specified, attempt a connection
    if let Some(leader_addr) = matches.get_one::<String>("leader") {
        let leader_addr = leader_addr.parse()?;
        if let Err(e) = worker
            .ctl
            .execute(bigworlds::rpc::worker::Request::ConnectToLeader(Some(leader_addr)).into())
            .await
        {
            error!("{e}");
        }
    }

    Ok(())
}
