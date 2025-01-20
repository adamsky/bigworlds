use anyhow::Result;
use clap::ArgMatches;
use tokio::runtime;

use bigworlds::{leader, net::CompositeAddress, util::Shutdown};

pub fn cmd() -> clap::Command {
    use clap::{builder::PossibleValue, Arg, Command};

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
        )
        .arg(
            Arg::new("autostep")
                .long("autostep")
                .help("Enable auto-stepping")
                .value_name("milliseconds"),
        )
}

/// Starts a leader task on the provided runtime.
pub async fn start(
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

    let autostep = matches.get_one::<String>("autostep").map(|a| {
        std::time::Duration::from_micros(
            a.parse()
                .expect("autostep value must be parsable into an integer"),
        )
    });

    let config = leader::Config {
        autostep,
        ..Default::default()
    };

    let handle = leader::spawn(listeners, config, runtime.clone(), shutdown.clone())?;

    Ok(())
}
