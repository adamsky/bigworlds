//! Simple example showing off the querying mechanism.

#![allow(warnings)]

use bigworlds::client::r#async::AsyncClient;
use bigworlds::client::ClientConfig;
use bigworlds::machine::cmd::get_set::Set;
use bigworlds::machine::Command;
use bigworlds::query::{Description, Filter, Map};
use bigworlds::util::Shutdown;
use bigworlds::{rpc, Executor, Model, Query, QueryProduct};
use messageio_client::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // initialize logging
    let _ =
        simplelog::SimpleLogger::init(simplelog::LevelFilter::Debug, simplelog::Config::default());

    let runtime = tokio::runtime::Handle::current();
    let shutdown = Shutdown::new();

    let mut sim = bigworlds::sim::spawn(runtime.clone(), shutdown.clone()).await?;
    sim.pull_model(Model::default()).await?;
    sim.initialize(None).await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let proc = sim
        .spawn_processor(|stream, exec| async move {
            let model = match exec.execute(rpc::worker::Request::Model).await?? {
                rpc::worker::Response::Model(model) => model,
                _ => unimplemented!(),
            };
            println!(">>> proc: model: {:?}", model);
            Ok(())
        })
        .await?;
    let machine = sim.spawn_machine().await?;

    // TODO
    // machine
    // .execute_script(
    //         r#"
    //         component health --var full,100 --var empty,0
    //         component stamina
    //         prototype monster -c health,stamina
    //         spawn monster
    //         "#,
    //     )
    //     .await?;

    let mut client = Client::connect(
        "127.0.0.1:9123".parse()?,
        ClientConfig::default(),
        runtime,
        shutdown,
    )
    .await?;

    // get health and stamina of all the entities marked as `monster`
    let query = Query::new()
        .description(Description::Addressed)
        .filter(Filter::Component("monster".into()))
        .map(Map::components(vec!["health", "stamina"]));

    let query_product = client.query(query).await?;

    if let QueryProduct::AddressedVar(product) = query_product {
        if product.is_empty() {
            println!("query product is empty");
        }
        for data_point in product {
            let (addr, var) = data_point;
            println!(
                "value of {} for entity {} is {:?}",
                addr.var_name, addr.entity, var
            );
        }
    }

    Ok(())
}
