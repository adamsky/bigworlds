use anyhow::Result;
use clap::ArgMatches;
use tokio::runtime;

use bigworlds::node::{self, NodeConfig};
use bigworlds::{net::CompositeAddress, util::Shutdown};

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
