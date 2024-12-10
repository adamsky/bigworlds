use std::env;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Error, Result};
use clap::ArgMatches;

use bigworlds::rpc;
use bigworlds::rpc::msg::UploadProjectRequest;
use bigworlds::util::Shutdown;
use bigworlds::util::{find_model_root, get_snapshot_paths};
use bigworlds::Executor;
use notify::Watcher;

use crate::interactive;
use crate::util::format_elements_list;

/// Starts a new simulation run, using a model or a snapshot file.
///
/// # Resolving ambiguity
///
/// If an explicit option for loading either model or snapshot is not
/// provided, this function makes appropriate selection based on directory
/// structure, file name and file content analysis.
///
/// If the path argument is not provided, the current working directory is
/// used.
pub async fn start(matches: &ArgMatches, shutdown: Shutdown) -> Result<()> {
    let mut path = env::current_dir()?;
    match matches.get_one::<String>("path") {
        Some(p_str) => {
            let p = PathBuf::from(p_str);

            // if provided path is relative, append it to current working directory
            if p.is_relative() {
                path = path.join(p);
            }
            // otherwise if it's absolute then just set it as the path
            else {
                path = p;
            }
        }
        // choose what to do if no path was provided
        None => {
            let root = find_model_root(path.clone(), 4)?;

            // TODO show all runnable options to the user

            if let Some(snapshot) = matches.get_one::<String>("snapshot") {
                let available = get_snapshot_paths(root)?;
                if available.len() == 1 {
                    return start_run_snapshot(available[0].clone(), matches, shutdown).await;
                } else if available.len() > 0 {
                    return Err(Error::msg(format!(
                        "choose one of the available snapshots: {}",
                        format_elements_list(&available)
                    )));
                } else {
                    return Err(Error::msg(format!("no snapshots available in project",)));
                }
            } else {
                unimplemented!()
            }
        }
    }

    path = path.canonicalize().unwrap_or(path);

    debug!("path: {:?}", path);

    if matches.get_one::<String>("scenario").is_none() {
        return start_run_model(path, matches, shutdown).await;
    } else if matches.get_one::<String>("snapshot").is_none() {
        return start_run_snapshot(path, matches, shutdown).await;
    } else {
        if path.is_file() {
            // decide whether the path looks more like scenario or snapshot
            if let Some(ext) = path.extension() {
                if ext == "toml" {
                    return start_run_model(path, matches, shutdown).await;
                }
            }
            return start_run_snapshot(path, matches, shutdown).await;
        }
        // path is provided but it's a directory
        else {
            let root = find_model_root(path.clone(), 4)?;

            // TODO allow choosing from available scenarios with additional
            // flag

            return start_run_model(root, matches, shutdown).await;
        }
    }

    Ok(())
}

async fn start_run_model(
    model_path: PathBuf,
    matches: &ArgMatches,
    shutdown: Shutdown,
) -> Result<()> {
    let model_root = find_model_root(model_path.clone(), 4)?;
    if matches.get_flag("interactive") {
        info!(
            "Running interactive session using scenario at: {:?}",
            model_path
        );

        let config_path = matches
            .get_one::<String>("icfg")
            .unwrap_or(&interactive::config::CONFIG_FILE.to_string())
            .to_string();

        let mut on_change = None;

        // store watcher here so it doesn't go out of scope
        let mut watcher: Option<notify::RecommendedWatcher> = None;

        if matches.get_one::<String>("watch").is_some() {
            use std::sync::Mutex;
            let watch_path = find_model_root(model_path.clone(), 4)?;
            info!(
                "watching changes at project path: {}",
                watch_path.to_string_lossy()
            );
            let change_detected = std::sync::Arc::new(Mutex::new(false));
            let change_detected_clone = change_detected.clone();
            let mut _watcher: notify::RecommendedWatcher =
                notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                    match res {
                        Ok(event) => {
                            // Disregard changes in build directories of cargo projects
                            // nested inside modules.
                            // TODO implement better filtering than basic word matching
                            if !event
                                .paths
                                .iter()
                                .any(|p| p.to_str().unwrap().contains("target"))
                            {
                                debug!("change detected: {:?}", event);
                                *change_detected_clone.lock().unwrap() = true;
                            } else {
                                trace!(
                                    "change detected, but ignored\
                                    (build directory?), paths: {:?}",
                                    event.paths
                                )
                            }
                        }
                        Err(e) => {
                            error!("watch error: {:?}", e);
                            *change_detected_clone.lock().unwrap() = true;
                        }
                    }
                })?;
            _watcher.watch(&watch_path, notify::RecursiveMode::Recursive)?;

            watcher = Some(_watcher);

            on_change = match matches.get_one::<String>("watch").map(|s| s.as_str()) {
                Some("restart") | None => Some(interactive::OnChange {
                    trigger: change_detected.clone(),
                    action: interactive::OnChangeAction::Restart,
                }),
                Some("update") => Some(interactive::OnChange {
                    trigger: change_detected.clone(),
                    action: interactive::OnChangeAction::UpdateModel,
                }),
                Some(action) => {
                    return Err(Error::msg(format!(
                        "not recognized change action for `watch`: {}",
                        action,
                    )))
                }
            };
        }

        let mut sim = bigworlds::sim::spawn_from_path(
            model_path,
            None,
            tokio::runtime::Handle::current(),
            shutdown.clone(),
        )
        .await?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        // TODO
        sim.server
            .ctl
            .execute(rpc::server::Request::Status.into())
            .await??;

        interactive::start(
            interactive::SimDriver::Local(sim),
            &config_path,
            on_change,
            shutdown,
        )
        .await?;
    }
    Ok(())
}

async fn start_run_snapshot(path: PathBuf, matches: &ArgMatches, shutdown: Shutdown) -> Result<()> {
    info!("Running interactive session using snapshot at: {:?}", path);

    // start the local sim task
    let mut sim =
        bigworlds::sim::spawn(tokio::runtime::Handle::current(), shutdown.clone()).await?;

    let response = sim
        .server
        .execute(
            UploadProjectRequest {
                archive: Vec::new(),
            }
            .into(),
        )
        .await?;

    if matches.get_flag("interactive") {
        interactive::start(
            interactive::SimDriver::Local(sim),
            matches
                .get_one::<String>("icfg")
                .unwrap_or(&interactive::config::CONFIG_FILE.to_string()),
            None,
            shutdown,
        );
    }
    Ok(())
}
