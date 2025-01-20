use anyhow::Result;
use clap::ArgMatches;
use tokio::runtime;

use bigworlds::node::{self, NodeConfig};
use bigworlds::{net::CompositeAddress, util::Shutdown};

pub fn cmd() -> clap::Command {
    use clap::{builder::PossibleValue, Arg, Command};

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
        )
}

pub async fn start(
    matches: &ArgMatches,
    runtime: runtime::Handle,
    shutdown: Shutdown,
) -> Result<()> {
    // extract the addresses to listen on
    let listeners = matches
        .get_one::<String>("listeners")
        .map(|s| s.split(',').collect::<Vec<&str>>())
        .map(|v| {
            v.iter()
                .filter_map(|s| s.parse::<CompositeAddress>().ok())
                .collect()
        })
        .unwrap_or(vec![CompositeAddress::available_net()?]);

    let mut config = NodeConfig::default();

    node::spawn(listeners, config, runtime.clone(), shutdown.clone())?;

    Ok(())
}
