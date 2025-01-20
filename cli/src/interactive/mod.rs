//! Defines an interactive interface for the command line.
//!
//! ## Local or remote
//!
//! `SimDriver` enum is used to differentiate between local and remote
//! modes. Local mode will operate directly on a `Sim` struct, while
//! remote mode will use a `Client` connected to an `bigworlds` server.
//! `Client` interface from the `bigworlds-net` crate is used.

mod compl;
pub mod config;
mod local;
mod remote;

mod img_print;

use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use std::{fs, io, thread};

use anyhow::Result;
use linefeed::inputrc::parse_text;
use linefeed::Signal;
use linefeed::{Interface, ReadResult};

use bigworlds::rpc::msg::Message;
use bigworlds::rpc::msg::{
    DataTransferRequest, SpawnEntitiesRequest, StatusRequest, TransferResponseData,
};
use bigworlds::{client::r#async::AsyncClient, string, util::Shutdown};
use bigworlds::{rpc, Executor};
use bigworlds::{sim, SimHandle};

use bigworlds::rpc::msg::LoadScenarioRequest;
use messageio_client::Client;

use self::compl::MainCompleter;

use crate::interactive::config::{Config, CONFIG_FILE};

pub enum SimDriver {
    Local(SimHandle),
    Remote(Client),
}

pub struct OnChange {
    pub trigger: Arc<Mutex<bool>>,
    pub action: OnChangeAction,
}

pub enum OnChangeAction {
    Restart,
    UpdateModel,
}

pub struct OnShutdown {
    pub trigger: Shutdown,
    pub action: OnShutdownAction,
}

pub enum OnShutdownAction {
    Custom,
    Quit,
}

/// Variant without the external change trigger.
pub async fn start_simple(driver: SimDriver, config_path: &str, shutdown: Shutdown) -> Result<()> {
    start(driver, config_path, None, shutdown).await
}

// TODO signal handling
/// Entry point for the interactive interface.
///
/// # Introducing runtime changes
///
/// Interactive interface supports updating simulation model or outright
/// restarting the simulation using an external trigger. This is used for
/// supporting a "watch" mode where changes to project files result in
/// triggering actions such as restarting the simulation using newly
/// introduced changes.
pub async fn start(
    mut driver: SimDriver,
    config_path: &str,
    on_change: Option<OnChange>,
    mut shutdown: Shutdown,
) -> Result<()> {
    'outer: loop {
        debug!("interactive: start outer");

        // check if there was a change triggered
        if let Some(ocm) = &on_change {
            let mut oc = ocm.trigger.lock().unwrap();
            if *oc == true {
                // switch off the trigger
                *oc = false;

                match ocm.action {
                    OnChangeAction::Restart => {
                        // send request to restart
                    }
                    _ => (),
                }

                continue;
            }
        }

        let interface = Arc::new(Interface::new("interactive")?);

        interface.set_completer(Arc::new(MainCompleter {
            // driver: driver_arc.clone(),
        }));

        // try loading config from file, else get a new default one
        let mut config = match Config::new_from_file(config_path) {
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    info!(
                        "Config file {} doesn't exist, loading default config settings",
                        CONFIG_FILE
                    );
                    Config::new()
                } else {
                    warn!("There was a problem parsing the config file, loading default config settings ({})", e);
                    Config::new()
                }
            }
            Ok(mut c) => {
                info!("Loading config settings from file (found {})", CONFIG_FILE);
                if c.turn_steps == 0 {
                    c.turn_steps = 1;
                }
                c
            }
        };

        match &mut driver {
            SimDriver::Local(sim) => {
                interface.set_prompt(local::create_prompt(&sim, &config).await.as_str())?;
            }
            SimDriver::Remote(client) => {
                let status = client.status().await?;
                let clock = status.current_tick;
                let str = format!(
                    "[{}] ",
                    //TODO this should instead ask for a default clock variable
                    // that's always available (right now it's using clock mod's var)
                    clock,
                );
                interface.set_prompt(&str);
                // remote::create_prompt(client, &config)
                //     .await
                //     .unwrap()
                //     .as_str()
            }
        };

        shutdown.try_recv();
        if shutdown.is_shutdown() {
            break 'outer;
        }

        let mut do_run = false;
        let mut do_run_loop = false;
        let mut do_run_freq = None;
        let mut last_time_insta = std::time::Instant::now();
        let mut just_left_loop = false;

        let mut run_loop_count = 0;

        interface.set_report_signal(Signal::Interrupt, true);
        interface.set_report_signal(Signal::Break, true);
        interface.set_report_signal(Signal::Quit, true);

        println!("\nYou're now in interactive mode.");
        println!("See possible commands with \"help\". Exit using \"quit\" or ctrl-d.");

        // start main loop
        loop {
            shutdown.try_recv();
            if shutdown.is_shutdown() {
                break 'outer;
            }

            // check for incoming network events
            // let mut driver = driver_arc.lock().unwrap();
            // TODO
            // match &driver.deref_mut() {
            //     SimDriver::Remote(ref mut client) => match client.try_recv() {
            //         Ok((_, event)) => match event.type_ {
            //             SocketEventType::Disconnect => {
            //                 println!("\nServer terminated the connection...");
            //                 break 'outer;
            //             }
            //             _ => (),
            //         },
            //         _ => (),
            //     },
            //     _ => (),
            // }

            if do_run_loop || do_run_freq.is_some() {
                if let Some(hz) = do_run_freq {
                    let target_delta_time = Duration::from_secs(1) / hz;
                    let time_now = std::time::Instant::now();
                    let delta_time = time_now - last_time_insta;
                    if delta_time >= target_delta_time {
                        last_time_insta = time_now;
                        do_run = true;
                    } else {
                        do_run = false;
                    }
                }

                if do_run_loop {
                    if run_loop_count <= 0 {
                        do_run = false;
                        do_run_loop = false;
                        run_loop_count = 0;
                    } else {
                        run_loop_count -= 1;
                        do_run = true;
                    }
                }

                // let mut driver = driver_arc.lock().unwrap();

                if do_run {
                    match &mut driver {
                        SimDriver::Local(sim) => {
                            sim.step().await?;
                            interface
                                .set_prompt(local::create_prompt(&sim, &config).await.as_str())?;
                        }
                        SimDriver::Remote(client) => {
                            let msg = client.step_request(1).await?;
                            interface
                                .set_prompt(create_prompt(&mut driver, &config).await?.as_str())?;
                        }
                    }

                    // // TODO signal functionality for stopping run_freq
                    // if shutdown.is_shutdown() {
                    //     do_run_freq = None;
                    //     do_run_loop = false;
                    //     continue;
                    // }

                    // if let Ok(res) = interface.read_line_step(Some(Duration::from_micros(1))) {
                    //
                    // } else {
                    //     continue;
                    // }
                    let read_result = match interface.read_line_step(Some(Duration::from_micros(1)))
                    {
                        Ok(res) => match res {
                            Some(r) => r,
                            None => continue,
                        },
                        Err(e) => continue,
                    };
                    match read_result {
                        _ => {
                            do_run = false;
                            do_run_freq = None;
                            do_run_loop = false;
                            run_loop_count = 0;
                            continue;
                        }
                    }
                }

                continue;
            }

            if let Some(res) = interface.read_line_step(Some(Duration::from_millis(10)))? {
                match res {
                    ReadResult::Input(line) => {
                        // let mut driver = driver_arc.lock().unwrap();

                        if !line.trim().is_empty() {
                            interface.add_history_unique(line.clone());
                        }

                        let (cmd, args) = split_first_word(&line);
                        match cmd {
                            "" => {
                                match &mut driver {
                                    SimDriver::Local(sim) => {
                                        if let Err(e) = local::process_turn(sim, &config).await {
                                            error!("{}", e);
                                        }
                                    }
                                    SimDriver::Remote(client) => {
                                        remote::process_step(client, &config).await?
                                    }
                                }
                                // interface
                                //     .set_prompt(create_prompt(&mut driver, &config)?.as_str())?;
                            }
                            "run" => {
                                do_run_loop = true;
                                run_loop_count = args.parse::<i32>().unwrap();
                                // interface.set_prompt("")?;
                            }
                            //TODO implement using clock int (it was using data string format
                            // before)
                            "run-until" => {
                                unimplemented!();
                            }
                            "runf" => {
                                interface.lock_reader();
                                let mut loop_count = args.parse::<u32>().unwrap();
                                match &mut driver {
                                    SimDriver::Local(sim) => {
                                        while loop_count > 0 {
                                            sim.step().await?;
                                            loop_count -= 1;
                                        }
                                    }
                                    SimDriver::Remote(client) => {
                                        unimplemented!()
                                        // client.server_step_request(loop_count)?;
                                    }
                                }
                                interface.set_prompt(
                                    create_prompt(&mut driver, &config).await?.as_str(),
                                )?;
                            }
                            //TODO
                            "runf-until" => {
                                unimplemented!();
                            }
                            "run-freq" => {
                                let hz = args.parse::<usize>().unwrap_or(10);
                                do_run_freq = Some(hz as u32);
                            }
                            "runf-hz" => {
                                let hz = args.parse::<usize>().unwrap_or(10);
                                let mut last = Instant::now();
                                loop {
                                    if Instant::now() - last
                                        < Duration::from_millis(1000 / hz as u64)
                                    {
                                        std::thread::sleep(Duration::from_millis(1));
                                        continue;
                                    }
                                    // TODO signal functionality for stopping run_freq
                                    if shutdown.is_shutdown() {
                                        do_run_freq = None;
                                        do_run_loop = false;
                                        break;
                                    }
                                    last = Instant::now();
                                    match &mut driver {
                                        SimDriver::Local(sim) => {
                                            sim.step().await?;
                                            interface.set_prompt(
                                                local::create_prompt(&sim, &config).await.as_str(),
                                            )?;
                                        }
                                        SimDriver::Remote(client) => {
                                            unimplemented!()
                                            // let msg = client.
                                            // server_step_request(1)?;
                                            // interface.set_prompt(
                                            //     create_prompt(&mut driver,
                                            // &config)?.as_str(),
                                            // )?;
                                        }
                                    }
                                }
                            }
                            // list variables
                            // TODO add option to not lookup entity names (faster)
                            "ls" | "l" => {
                                let query = bigworlds::Query::default()
                                    .description(bigworlds::query::Description::Addressed);
                                let product = match driver {
                                    SimDriver::Local(ref sim_handle) => {
                                        sim_handle.query(query).await?
                                    }
                                    SimDriver::Remote(ref mut client) => {
                                        client.query(query).await?
                                    }
                                };

                                for (addr, var) in product.to_map()? {
                                    println!("{addr} = {var:?}");
                                }
                            }
                            // spawn entity
                            "spawn" => {
                                let split = args.split(" ").collect::<Vec<&str>>();
                                match &driver {
                                    SimDriver::Remote(client) => {
                                        unimplemented!()
                                        // client.connection.send_payload(
                                        //     SpawnEntitiesRequest {
                                        //         entity_prefabs: vec![split[0].to_string()],
                                        //         entity_names: vec![split[1].to_string()],
                                        //     },
                                        //     None,
                                        // )?;
                                        // client.connection.recv_msg()?;
                                    }
                                    SimDriver::Local(sim) => {
                                        unimplemented!();
                                        // sim.spawn_entity_by_prefab_name(
                                        //     Some(&string::new_truncate(split[0])),
                                        //     Some(string::new_truncate(split[1])),
                                        // )?;
                                    }
                                }
                            }
                            "nspawn" => {
                                let split = args.split(" ").collect::<Vec<&str>>();

                                unimplemented!();

                                // match &mut sim_driver {
                                //     SimDriver::Local(sim) => {
                                //         for n in 0..split[0].parse().unwrap() {
                                //             let prefab = match split.get(1) {
                                //                 Some(s) => {
                                //                     sim.spawn_entity_by_prefab_name(
                                //                         Some(&string::new_truncate(s)),
                                //                         None,
                                //                     )?;
                                //                 }
                                //                 None => {
                                //                     sim.spawn_entity(None, None)?;
                                //                 }
                                //             };
                                //         }
                                //     }
                                //     SimDriver::Remote(client) => {
                                //         unimplemented!()
                                //         // client.connection.send_payload(
                                //         //     SpawnEntitiesRequest {
                                //         //         entity_prefabs: vec![split[0].to_string()],
                                //         //         entity_names: vec![split[1].to_string()],
                                //         //     },
                                //         //     None,
                                //         // )?;
                                //         // client.connection.recv_msg()?;
                                //     }
                                // }
                            }
                            // Write an uncompressed snapshot to disk.
                            "snap" => {
                                if args.contains(" ") {
                                    println!("Snapshot file path cannot contain spaces.");
                                    continue;
                                }

                                unimplemented!();

                                // match &mut sim_driver {
                                //     SimDriver::Local(sim) => match sim.save_snapshot(args, false) {
                                //         Ok(d) => d,
                                //         Err(e) => {
                                //             println!("{}", e);
                                //             continue;
                                //         }
                                //     },
                                //     SimDriver::Remote(client) => {
                                //         unimplemented!()
                                //         // match client.snapshot_request(args.to_string(), true) {
                                //         //     Ok(snapshot) => {
                                //         //         println!(
                                //         //             "received snapshot len: {}",
                                //         //             snapshot.len()
                                //         //         );
                                //         //     }
                                //         //     Err(e) => {
                                //         //         error!("{}", e);
                                //         //         continue;
                                //         //     }
                                //         // }
                                //     }
                                //     _ => unimplemented!(),
                                // };
                            }
                            // Write a compressed snapshot to disk.
                            "snapc" => {
                                if args.contains(" ") {
                                    println!("Snapshot file path cannot contain spaces.");
                                    continue;
                                }

                                unimplemented!();

                                // let data = match &mut sim_driver {
                                //     SimDriver::Local(sim) => match sim.save_snapshot(args, true) {
                                //         Ok(d) => d,
                                //         Err(e) => {
                                //             println!("{}", e);
                                //             continue;
                                //         }
                                //     },
                                //     _ => unimplemented!(),
                                // };
                            }
                            "init" => {
                                // initialize the cluster with default model
                                match driver {
                                    SimDriver::Local(sim_handle) => todo!(),
                                    SimDriver::Remote(ref mut client) => {
                                        client.send_msg(&Message::InitializeRequest)?;
                                        if let Message::InitializeResponse =
                                            client.recv_msg().await?
                                        {
                                            println!("initialized succesfully");
                                        } else {
                                            error!("not initialize response");
                                        }
                                    }
                                }
                            }

                            "help" => {
                                println!("available commands:");
                                println!();
                                for &(cmd, help) in APP_COMMANDS {
                                    println!("  {:15} - {}", cmd, help);
                                }
                                println!();
                            }
                            "cfg-list" => {
                                println!(
                                    "\n\
turn_ticks              {turn_ticks}
show_on                 {show_on}
show_list               {show_list}
",
                                    turn_ticks = config.turn_steps,
                                    show_on = config.show_on,
                                    show_list = format!("{:?}", config.show_list),
                                );
                            }
                            "cfg-get" => match config.get(args) {
                                Err(e) => println!("Error: {} doesn't exist", args),
                                Ok(c) => println!("{}: {}", args, c),
                            },
                            "cfg" => {
                                let (var, val) = split_first_word(&args);
                                match config.set(var, val) {
                                    Err(e) => println!("Error: couldn't set {} to {}", var, val),
                                    Ok(()) => println!("Setting {} to {}", var, val),
                                }
                            }
                            "cfg-save" => {
                                println!("Exporting current configuration to file {}", CONFIG_FILE);
                                config.save_to_file(CONFIG_FILE).unwrap();
                            }
                            "cfg-reload" => {
                                config = match Config::new_from_file(CONFIG_FILE) {
                                    Err(e) => {
                                        if e.kind() == io::ErrorKind::NotFound {
                                            println!(
                                                "Config file {} doesn't exist, loading default config settings",
                                                CONFIG_FILE
                                            );
                                            Config::new()
                                        } else {
                                            eprintln!("There was a problem parsing the config file, loading default config settings. Details: {}", e);
                                            Config::new()
                                        }
                                    }
                                    Ok(c) => {
                                        println!(
                                            "Successfully reloaded configuration settings (found {})",
                                            CONFIG_FILE
                                        );
                                        c
                                    }
                                };
                            }

                            "show-grid" => {
                                let args = args.split(' ').collect::<Vec<&str>>();
                                match &mut driver {
                                    // let addr =
                                    SimDriver::Local(sim) => {
                                        // local::print_show_grid(&sim, &config, args[0])
                                    }
                                    SimDriver::Remote(client) => remote::print_show_grid(
                                        client,
                                        &config,
                                        args[0],
                                        args.get(1).unwrap_or(&"1.").parse().unwrap(),
                                    ),
                                    _ => unimplemented!(),
                                }
                            }

                            "show" => match &driver {
                                SimDriver::Local(sim) => local::print_show(&sim, &config).await,
                                _ => unimplemented!(),
                            },

                            "show-toggle" => {
                                if config.show_on {
                                    config.show_on = false
                                } else {
                                    config.show_on = true
                                };
                            }

                            "show-add" => {
                                // TODO handle unwrap
                                config.show_add(args).unwrap();
                            }

                            "show-remove" => {
                                // TODO handle unwrap
                                config.show_remove(args).unwrap();
                            }

                            "show-clear" => {
                                config.show_list.clear();
                            }

                            "history" => {
                                let w = interface.lock_writer_erase()?;

                                for (i, entry) in w.history().enumerate() {
                                    println!("{}: {}", i, entry);
                                }
                            }

                            "quit" => break 'outer,

                            // hidden commands
                            "interface-set" => {
                                let d = parse_text("<input>", &line);
                                interface.evaluate_directives(d);
                            }
                            "interface-get" => {
                                if let Some(var) = interface.get_variable(args) {
                                    println!("{} = {}", args, var);
                                } else {
                                    println!("no variable named `{}`", args);
                                }
                            }
                            "interface-list" => {
                                for (name, var) in interface.lock_reader().variables() {
                                    println!("{:30} = {}", name, var);
                                }
                            }
                            "spawn" => {
                                let args = args.split(' ').collect::<Vec<&str>>();
                                let prefab = args[0].to_string();
                                let name = args[1].to_string();
                                match &driver {
                                    SimDriver::Local(sim) => {
                                        sim.spawn_entity(name, prefab).await?;
                                    }
                                    _ => (),
                                }
                            }
                            "model" => match &mut driver {
                                SimDriver::Local(sim) => {
                                    println!("{:#?}", sim.model().await?);
                                }
                                _ => (),
                            },
                            "disco" => match &mut driver {
                                SimDriver::Local(sim) => unimplemented!(),
                                SimDriver::Remote(client) => {
                                    client.send_msg(&Message::Disconnect)?
                                }
                            },

                            _ => println!("couldn't recognize input: {:?}", line),
                        }
                        if do_run_freq.is_none() && !do_run_loop {
                            interface
                                .set_prompt(create_prompt(&mut driver, &config).await?.as_str())?;
                        }

                        // std::mem::drop(driver);
                    }
                    // handle quitting using signals and eof
                    ReadResult::Signal(Signal::Break)
                    | ReadResult::Signal(Signal::Interrupt)
                    | ReadResult::Eof => {
                        // println!("do_run: {:?}", do_run);
                        // println!("do_run_freq: {:?}", do_run_freq);
                        // println!("do_run_loop: {:?}", do_run_loop);
                        interface.cancel_read_line();
                        do_run = false;
                        do_run_freq = None;
                        do_run_loop = false;
                        run_loop_count = 0;
                        if do_run_freq.is_none() && !do_run_loop {
                            break 'outer;
                            // break;
                        }
                        // interface.set_prompt(create_prompt(&mut driver, &config).as_str())?;
                    }
                    _ => println!("err"),
                }
            }

            // check remote trigger
            if let Some(oc) = &on_change {
                let mut c = oc.trigger.lock().unwrap();
                if *c == true {
                    interface.cancel_read_line();
                    match oc.action {
                        OnChangeAction::Restart => {
                            warn!("changes to project files detected, restarting...");
                            continue 'outer;
                        }
                        OnChangeAction::UpdateModel => {
                            warn!(
                                "changes to project files detected, \
                                updating current simulation instance model..."
                            );
                            // if let SimDriver::Local(sim) = driver_arc.lock().unwrap().deref_mut()
                            // {
                            if let SimDriver::Local(sim) = driver {
                                unimplemented!();
                                // instantiate new sim instance and step it once to initialize
                                // let mut new_sim =
                                //     SimHandle::from_scenario_at(&path.as_ref().unwrap())?;
                                // new_sim.step()?;
                                //
                                // println!("applying new model");
                                // // apply the new model
                                // sim.model = new_sim.model;
                                // println!("done");
                            }
                            continue 'outer;

                            // let new_model;
                        }
                    }
                }
            }

            // process client
            if let SimDriver::Remote(ref mut client) = driver {
                // unimplemented!()
                // client.manual_poll().unwrap();
            }
        }
        if let SimDriver::Remote(ref mut client) = driver {
            println!("Disconnecting...");
            client.send_msg(&Message::Disconnect)?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            client.disconnect().await;
            // thread::sleep(Duration::from_millis(500));
        }
    }
    println!("Leaving interactive mode.");
    // send the shutdown signal
    shutdown.shutdown()?;
    match driver {
        SimDriver::Local(sim_handle) => {
            sim_handle.shutdown().await?;
        }

        SimDriver::Remote(ref mut client) => {
            println!("Disconnecting...");
            client.disconnect();
        }
    }
    Ok(())
}

pub async fn create_prompt(mut driver: &mut SimDriver, cfg: &Config) -> Result<String> {
    match &mut driver {
        SimDriver::Local(sim) => Ok(local::create_prompt(&sim, &cfg).await),
        SimDriver::Remote(client) => remote::create_prompt(client, &cfg).await,
    }
}

static APP_COMMANDS: &[(&str, &str)] = &[
    ("run", "Run a number of simulation ticks (hours), takes in an integer number"),
    ("runf", "Similar to `run` but doesn't listen to interupt signals, `f` stands for \"fast\" \
        (it's faster, but you will have to wait until it's finished processing)"),
    ("run-freq", "Run simulation at a constant pace, using the provided frequency"),
    ("test", "Run quick mem+proc test. Takes in a number of secs to run the average processing speed test (default=2)"),
    ("ls", "List simple variables (no lists or grids). Takes in a string argument, returns only vars that contain that string in their address"),
    ("snap", "Export current sim state to snapshot file. Takes a path to target file, relative to where endgame is running."),
    ("snapc", "Same as snap but applies compression"),
    ("cfg", "Set config variable"),
    ("cfg-get", "Print the value of one config variable"),
    ("cfg-list", "Get a list of all config variables"),
    ("cfg-save", "Save current configuration to file"),
    ("cfg-reload", "Reload current configuration from file"),
    ("show", "Print selected simulation data"),
    ("show-add", "Add to the list of simulation data to be shown"),
    (
        "show-remove",
        "Remove from the list of simulation data to be shown (by index, starting at 0)",
    ),
    (
        "show-clear",
        "Clear the list of simulation data to be shown",
    ),
    ("show-toggle", "Toggle automatic printing after each turn"),
    ("history", "Print input history"),
    ("help", "Show available commands"),
    (
        "quit",
        "Quit (NOTE: all unsaved data will be lost)",
    ),
];

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim();

    match s.find(|ch: char| ch.is_whitespace()) {
        Some(pos) => (&s[..pos], s[pos..].trim_start()),
        None => (s, ""),
    }
}
