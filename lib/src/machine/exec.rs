//! This module defines functionalist for dealing with executing command
//! collections within different contexts

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use fnv::FnvHashMap;
use itertools::Itertools;

use crate::entity::{Entity, Storage};
use crate::executor::Executor;
use crate::machine::{ErrorKind, Result};
use crate::{Address, CompName, EntityId, EntityName, Model, StringId};

#[cfg(feature = "machine_dynlib")]
use crate::machine::Libraries;

use super::cmd::{CentralRemoteCommand, Command, CommandResult, ExtCommand};
use super::error::Error;
use super::{CallStackVec, ExecutionContext, LocationInfo, Registry};

/// Executes a given set of central-external commands.
//TODO missing component uid information
pub(crate) fn execute_central_ext(
    central_ext_cmds: &Vec<(ExecutionContext, CentralRemoteCommand)>,
    // sim: &mut SimExec,
) -> Result<()> {
    // println!("central_ext_cmds: {:#?}", central_ext_cmds);

    // // reorganize commands based on entity id
    // let mut ents = BTreeMap::new();
    // for (exe_loc, central_ext_cmd) in central_ext_cmds {
    //     if !ents.contains_key(&exe_loc.ent) {
    //         ents.insert(exe_loc.ent, Vec::new());
    //     } else {
    //         ents.get_mut(&exe_loc.ent)
    //             .unwrap()
    //             .push((exe_loc.clone(), central_ext_cmd.clone()))
    //     }
    // }
    //
    // for (k, v) in ents.into_iter().sorted_by_key(|(k, _)| *k).rev() {
    //     println!("{}: {:#?}", k, v);
    //     for (exe_loc, central_ext_cmd) in v {
    //         if let Err(me) = central_ext_cmd.execute(sim, &exe_loc.ent,
    // &exe_loc.comp) {             error!("{}", me);
    //         }
    //     }
    // }

    for (exe_loc, central_ext_cmd) in central_ext_cmds {
        // if let Err(me) = central_ext_cmd.execute(sim, &exe_loc.comp) {
        //     error!("{}", me);
        // }
    }

    Ok(())
}

/// Executes a given set of external commands.
//TODO missing component uid information
pub(crate) fn execute_ext(
    ext_cmds: &Vec<(ExecutionContext, ExtCommand)>,
    // sim: &mut SimExec,
) -> Result<()> {
    for (exec_ctx, ext_cmd) in ext_cmds {
        // if let Err(e) = ext_cmd.execute(sim, &exec_ctx.comp, &exec_ctx.location) {
        //     error!("{}", e);
        // }
    }
    Ok(())
}

/// Executes a given set of commands within a local entity scope.
///
/// Most of the errors occurring during execution of commands are non-breaking.
/// If a breaking error occurs this function will itself return an `Error`
/// containing the reason and location info of the appropriate command.
/// If no breaking errors occur it returns `Ok`.
///
/// ### External command collection arguments
///
/// Arguments include references to atomically counted collections of `ext` and
/// `central_ext` commands. This is because some commands executed on the
/// entity level that are targeting execution in a higher context will yield
/// command results containing either `ext` or `central_ext` commands.
///
/// ### Optional start and end line arguments
///
/// Execution can optionally be restricted to a subset of all commands using
/// the start and end line numbers. This is used when executing a selected
/// state, since states are essentially described using their start and end
/// line numbers.
pub(crate) async fn execute_loc(
    cmds: &Vec<Command>,
    locations: &Vec<LocationInfo>,
    mut ent_storage: &mut Storage,
    mut comp_state: &mut StringId,
    ent_uid: &EntityId,
    comp_name: &CompName,
    start: Option<usize>,
    end: Option<usize>,
    // exec: E,
) -> Result<()> {
    trace!(
        "execute_loc (start:{:?}, end:{:?}): cmds: {:?}",
        start,
        end,
        cmds,
    );

    // initialize a new call stack
    let mut call_stack = CallStackVec::new();
    let mut registry = Registry::new();
    let mut cmd_n = match start {
        Some(s) => s,
        None => 0,
    };
    // let mut cexts = Vec::new();
    // let mut exts = Vec::new();
    'outer: loop {
        if cmd_n >= cmds.len() {
            break;
        }
        if let Some(e) = end {
            if call_stack.is_empty() && cmd_n >= e {
                break;
            }
        }
        let loc_cmd = cmds.get(cmd_n).unwrap();
        let location_info = locations.get(cmd_n).ok_or(Error::new(
            LocationInfo::empty(),
            ErrorKind::Other(format!(
                "location info not found for command: {:?}",
                loc_cmd
            )),
        ))?;
        trace!("command: {:?}", loc_cmd);
        trace!("command location_info: {:?}", location_info);
        unimplemented!()
        // let mut comp = entity.components.get_mut(&comp_uid).unwrap();
        // match loc_cmd
        //     .execute(
        //         &mut ent_storage,
        //         &mut comp_state,
        //         &mut call_stack,
        //         &mut registry,
        //         comp_name,
        //         ent_uid,
        //         location_info,
        //         // exec.clone(),
        //     )
        //     .await
        // {
        //     CommandResult::Continue => (),
        //     CommandResult::Break => break 'outer,
        //     CommandResult::JumpToLine(n) => {
        //         cmd_n = n;
        //         continue 'outer;
        //     }
        //     CommandResult::JumpToTag(t) => {
        //         if let Some((line, _)) = locations
        //             .iter()
        //             .enumerate()
        //             .find(|(_, l)| l.tag == Some(t.clone()))
        //         {
        //             cmd_n = line;
        //             continue 'outer;
        //         }
        //     }
        //     // CommandResult::ExecExt(ext_cmd) => {
        //     //     // push external command to aggregate vec
        //     //     exts.push((
        //     //         ExecutionContext {
        //     //             ent: *ent_uid,
        //     //             comp: comp_uid.clone(),
        //     //             location: location_info.clone(),
        //     //         },
        //     //         ext_cmd,
        //     //     ));
        //     // }
        //     // CommandResult::ExecCentralExt(cext_cmd) => {
        //     //     // push central external command to aggregate vec
        //     //     cexts.push((
        //     //         ExecutionContext {
        //     //             ent: *ent_uid,
        //     //             comp: comp_uid.clone(),
        //     //             location: location_info.clone(),
        //     //         },
        //     //         cext_cmd,
        //     //     ));
        //     // }
        //     CommandResult::Err(e) => {
        //         //TODO implement configurable system for deciding whether to
        //         // break state, panic or just print when given error occurs
        //         error!("{}", e);
        //     }
        // }
        // cmd_n += 1;
    }
    // central_ext_cmds.lock().unwrap().extend(cexts);
    // ext_cmds.lock().unwrap().extend(exts);
    Ok(())
}

// /// Executes given set of commands within global sim scope.
// pub async fn execute(
//     cmds: &Vec<Command>,
//     ent_id: &EntityId,
//     comp_uid: &CompName,
//     model: &SimModel,
//     mut sim: &mut SimExec,
//     start: Option<usize>,
//     end: Option<usize>,
//     #[cfg(feature = "machine_dynlib")] libs: &Libraries,
// ) -> Result<()> {
//     // initialize a new call stack
//     let mut call_stack = CallStackVec::new();
//     let mut registry = Registry::new();
//
//     let mut empty_locinfo = LocationInfo::empty();
//     empty_locinfo.line = Some(0);
//
//     let mut cmd_n = match start {
//         Some(s) => s,
//         None => 0,
//     };
//     'outer: loop {
//         if cmd_n >= cmds.len() {
//             break;
//         }
//         if let Some(e) = end {
//             if call_stack.is_empty() && cmd_n >= e {
//                 break;
//             }
//         }
//         let loc_cmd = cmds.get(cmd_n).unwrap();
//         let location = model
//             .get_component(comp_uid)
//             .expect("can't get component model")
//             .logic
//             .cmd_location_map
//             .get(cmd_n)
//             .unwrap_or(&empty_locinfo)
//             .clone();
//
//         // let entity = sim.entities.get_mut(sim.entities_idx.get(ent_uid).unwrap());
//         let mut comp_state: StringId = sim
//             .execute(Signal::EntityCompStateRequest(
//                 ent_id.clone(),
//                 comp_uid.clone(),
//             ))
//             .await??
//             .into_entity_comp_state_response()?;
//         let result = loc_cmd
//             .execute(
//                 &mut entity.storage,
//                 &mut entity.insta,
//                 &mut comp_state,
//                 &mut call_stack,
//                 &mut registry,
//                 comp_uid,
//                 ent_id,
//                 &model,
//                 &location,
//                 #[cfg(feature = "machine_dynlib")]
//                 libs,
//             )
//             .await;
//         match result {
//             CommandResult::Continue => (),
//             CommandResult::Break => break 'outer,
//             CommandResult::JumpToLine(n) => {
//                 cmd_n = n;
//                 continue 'outer;
//             }
//             CommandResult::JumpToTag(_) => unimplemented!("jumping to tag not supported"),
//             // CommandResult::ExecExt(ext_cmd) => {
//             //     ext_cmd.execute(sim, ent_id, comp_uid, &location)?;
//             // }
//             // CommandResult::ExecCentralExt(cext_cmd) => {
//             //     cext_cmd.execute(sim, ent_id, comp_uid)?;
//             // }
//             CommandResult::Err(e) => {
//                 //TODO implement configurable system for deciding whether to
//                 // break state, panic or just print when given error occurs
//                 error!("{}", e);
//             }
//         }
//         cmd_n += 1;
//     }
//     Ok(())
// }
