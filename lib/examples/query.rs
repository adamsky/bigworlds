//! Simple example showing off the querying mechanism.

#![allow(warnings)]

use std::str::FromStr;

use bigworlds::address::LocalAddress;
use bigworlds::client::r#async::AsyncClient;
use bigworlds::client::ClientConfig;
use bigworlds::machine::cmd::get_set::Set;
use bigworlds::machine::Command;
use bigworlds::query::{Description, Filter, Map};
use bigworlds::util::Shutdown;
use bigworlds::{model, Address, Var, VarType};
use bigworlds::{rpc, Executor, Model, Query, QueryProduct};
use messageio_client::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // initialize logging
    // let _ =
    //     simplelog::SimpleLogger::init(simplelog::LevelFilter::Debug, simplelog::Config::default());

    let shutdown = Shutdown::new();

    let mut sim = bigworlds::sim::spawn(shutdown.clone()).await?;

    println!("sim spawned");

    // define a basic model
    let mut model = Model::default();
    model.components.push(model::Component {
        name: "health".to_string(),
        vars: vec![model::Var {
            name: "current".to_string(),
            type_: VarType::Int,
            default: Some(Var::Int(rand::random_range(1..100))),
        }],
    });
    model.components.push(model::Component {
        name: "stamina".to_string(),
        vars: vec![model::Var {
            name: "current".to_string(),
            type_: VarType::Float,
            default: Some(Var::Float(rand::random_range(5.0..50.0))),
        }],
    });
    model.prefabs.push(model::PrefabModel {
        name: "monster".to_string(),
        components: vec!["health".to_string(), "stamina".to_string()],
    });
    sim.pull_model(model).await?;

    sim.initialize(None).await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    for name in ["ghoul", "wyvern", "troll"] {
        sim.spawn_entity(name.to_string(), "monster".to_string())
            .await?;
    }

    sim.set_var(
        "ghoul:stamina:float:current".parse()?,
        Var::from_str("101.0", Some(VarType::Float))?,
    )
    .await?;

    let proc = sim
        .spawn_behavior(|stream, exec| {
            Box::pin(async move {
                // modify some data point each second
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let product: QueryProduct = exec
                        .execute(rpc::worker::Request::Query(
                            Query::default()
                                .filter(Filter::Name(vec!["ghoul".to_string()]))
                                .map(Map::Components(vec!["stamina".to_string()])),
                        ))
                        .await??
                        .try_into()?;
                    println!("behavior got product");
                    let mut product = product.to_vec();
                    let var = product.pop().unwrap();
                    let var = Var::Float(var.to_float() + 1.);

                    exec.execute(rpc::worker::Request::SetVar(
                        "ghoul:stamina:float:current".parse()?,
                        var,
                    ))
                    .await?;
                }

                // let model = match exec.execute(rpc::worker::Request::Model).await?? {
                //     rpc::worker::Response::Model(model) => model,
                //     _ => unimplemented!(),
                // };
                // println!(">>> proc: model: {:?}", model);

                Ok(())
            })
        })
        .await?;

    let mut client = Client::connect(
        "127.0.0.1:9123".parse()?,
        ClientConfig::default(),
        tokio::runtime::Handle::current(),
        shutdown,
    )
    .await?;

    // get health and stamina of all the entities marked as `monster`
    let query = Query::default()
        .description(Description::Addressed)
        .filter(Filter::VarRange(
            LocalAddress::from_str("health:int:current")?,
            Var::Int(12),
            Var::Int(90),
        ))
        .map(Map::components(vec!["health", "stamina"]));

    for n in 0..5 {
        println!("querying client");

        let query_product = client.query(query.clone()).await?;

        println!("got product");

        if let QueryProduct::AddressedVar(product) = query_product {
            if product.is_empty() {
                println!("query product is empty");
            }
            for data_point in product {
                let (addr, var) = data_point;
                println!("{}: {:?}", addr, var);
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    sim.shutdown().await?;

    Ok(())
}
