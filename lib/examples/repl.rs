#![allow(warnings)]

//! This example shows how to quickly put together a simple interactive command
//! line interpreter using the building blocks provided by the library.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, io};

use anyhow::{anyhow, Result};
use simplelog::{ColorChoice, Config, ConfigBuilder, LevelFilter, TermLogger, TerminalMode};
use tokio::runtime;
use vfs::PhysicalFS;

use bigworlds::machine::{cmd, exec, script, LocationInfo};
use bigworlds::util::Shutdown;
use bigworlds::{sim, Model};

#[tokio::main]
async fn main() -> Result<()> {
    // initialize logging
    let _ =
        simplelog::SimpleLogger::init(simplelog::LevelFilter::Trace, simplelog::Config::default());

    // load project
    let path = match env::args().into_iter().nth(1) {
        // support providing custom path
        Some(p) => format!("{}/{}", env::current_dir().unwrap().to_str().unwrap(), p),
        // fall back on the project provided in examples
        None => format!("{}/examples/project", env!("CARGO_MANIFEST_DIR")),
    };
    let model = Model::from_files(&PhysicalFS::new(path), None)?;

    let runtime = tokio::runtime::Handle::current();
    let shutdown = Shutdown::new();

    let mut sim = match sim::spawn_from(model, None, runtime, shutdown).await {
        Ok(s) => s,
        Err(e) => {
            return Err(anyhow::anyhow!("failed spawning sim: {}", e));
        }
    };

    println!("entities: {:?}", sim.entities().await?);

    // spawn new machine behavior for executing commands
    let machine = sim.spawn_machine().await?;

    // support amalgamating input using escape sequence `\\`
    let mut input_amalg = String::new();

    // main repl loop
    'outer: loop {
        // read user input
        print!(">");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("failed to read from stdin");

        if input.trim().ends_with("\\") {
            input_amalg.push_str(&input);
            continue;
        } else if !input_amalg.is_empty() {
            input_amalg.push_str(&input);
            input = input_amalg.clone();
            input_amalg = String::new();
        }

        // parse input as instructions, potentially multiple lines
        let instructions = match script::parser::parse_lines(&input, LocationInfo::empty()) {
            Ok(i) => i,
            Err(e) => {
                println!("{:?}", e);
                continue;
            }
        };

        // get only command prototypes
        let mut cmd_protos = Vec::new();
        for instr in instructions {
            match instr.type_ {
                script::InstructionType::Command(cp) => cmd_protos.push(cp),
                _ => {
                    println!("not a command");
                    continue 'outer;
                }
            };
        }

        // generate runnable commands
        let mut commands = Vec::new();
        for (n, cmd_proto) in cmd_protos.iter().enumerate() {
            let mut location = LocationInfo::empty();
            location.line = Some(n);
            let command = match cmd::Command::from_prototype(&cmd_proto, &location, &cmd_protos) {
                Ok(c) => c,
                Err(e) => {
                    println!("{:?}", e);
                    continue 'outer;
                }
            };
            machine.execute_cmd(command.clone()).await.map(|_| ())?;
            commands.push(command);
        }

        // execute commands on the machine behavior
        // machine.execute_cmd_batch(commands).await.map(|_| ())?;

        // TODO: remove
        // exec::execute_loc(&commands, &ent_uid, &comp_uid, &mut sim, None, None).unwrap();
    }
}

fn init_logger() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    // find out whether to print development logs
    // by default we only print logs from setup and from runtime
    // print commands
    let do_devlog = args.contains(&"--verbose".to_string());

    // setup the logger
    if !do_devlog {
        // add custom config to apply some filters
        let custom_config = ConfigBuilder::new()
            .add_filter_allow_str("bigworlds::cmd::print")
            .add_filter_allow_str("bigworlds::script::preprocessor")
            .build();
        TermLogger::init(
            LevelFilter::max(),
            custom_config,
            TerminalMode::Mixed,
            ColorChoice::Auto,
        )
        .unwrap();
    } else {
        let default_config = Config::default();
        TermLogger::init(
            LevelFilter::max(),
            default_config,
            TerminalMode::Mixed,
            ColorChoice::Auto,
        )
        .unwrap();
    }

    Ok(())

    // // handle path to scenario
    // let path = match env::args().into_iter().nth(1) {
    //     Some(p) => p,
    //     None => {
    //         return Err(anyhow!("Please provide a path to an existing scenario"));
    //     }
    // };
    // let current_path = env::current_dir().expect("failed getting current dir path");
    // let path_buf = PathBuf::from(path).canonicalize().unwrap();
    // if !path_buf.exists() {
    //     // return Err("Please provide a path to an existing scenario".to_string()).into();
    // }
    // let path_to_scenario = current_path.join(path_buf);

    // instantiate simulation
    // let (model, mut sim) = match
    // Sim::from_scenario_at(path_to_scenario) {
    // match engine::sim::spawn
    // match SimExec::from_scenario_at_path(path_to_scenario.clone()) {
    //     Ok(s) => Ok(s),
    //     Err(e) => {
    //         return Err(anyhow!(
    //             "failed making sim from scenario at path: {}: {}",
    //             path_to_scenario.to_str().unwrap(),
    //             e,
    //         ));
    //     }
    // }
}
