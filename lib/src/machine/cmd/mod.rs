//! Command definitions.
//!
//! Command struct serves as the basic building block for the in-memory logic
//! representation used by the runtime. Each command provides an implementation
//! for it's interpretation (converting a command prototype into target struct)
//! and for it's execution (performing work, usually work on some piece of
//! data).
//!
//! Individual *component behaviors*, as seen declared within a model, exist
//! as collections of command structs. During logic execution, these
//! collections are iterated on, executing the commands one by one.
//!
//! Command structs are stored on component models, making each component of
//! certain type contain the same set of commands.

#![allow(unused)]

use std::collections::{BTreeMap, HashMap};
use std::env::args;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use arrayvec::ArrayVec;
use fnv::FnvHashMap;
// use serde_yaml::Value;
use smallvec::SmallVec;

#[cfg(feature = "machine_dynlib")]
use libloading::Library;

use crate::address::{Address, ShortLocalAddress};
use crate::{string, CompName, EntityId, EntityName, ShortString, StringId, Var, VarType};

use crate::{model, util};

use crate::entity::{Entity, Storage};
// use crate::error::Error;
use crate::model::Model;
// use crate::Result;

pub mod print;
pub mod register;
// pub mod equal;
// pub mod attach;
// pub mod detach;
pub mod eval;
pub mod flow;
pub mod get_set;
pub mod goto;
pub mod invoke;
// pub mod jump;
pub mod spawn;
// pub mod range;
pub mod set;
// pub mod sim;
pub mod query;

// #[cfg(feature = "machine_dynlib")]
// pub mod lib;
// #[cfg(feature = "machine_dynlib")]
// use crate::machine::cmd::lib::LibCall;

// use self::equal::*;
// use self::eval::*;
// use self::get_set::*;
// use self::lib::*;

use crate::executor::{Executor, LocalExec};
use crate::machine::cmd::CommandResult::JumpToLine;
use crate::machine::error::{Error, ErrorKind, Result};
use crate::machine::{CommandPrototype, CommandResultVec, ExecutionContext, LocationInfo};

use super::Machine;

/// Used for controlling the flow of execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandResult {
    /// Continue execution
    Continue,
    /// Break execution
    Break,
    /// Jump to line
    JumpToLine(usize),
    /// Jump to tag
    JumpToTag(StringId),
    // /// Execute command that needs access to another entity
    // ExecExt(ExtCommand),
    // /// Execute command that needs access to central authority
    // ExecCentralExt(CentralRemoteCommand),
    /// Signalize that an error has occurred during execution of command
    Err(Error),
}

impl CommandResult {
    pub fn from_str(s: &str) -> Option<CommandResult> {
        if s.starts_with("jump.") {
            let c = &s[5..];
            return Some(CommandResult::JumpToLine(c.parse::<usize>().unwrap()));
        }
        //else if s.starts_with("goto.") {
        //let c = &s[5..];
        //return Some(CommandResult::GoToState(SmallString::from_str(c).unwrap()));
        //}
        else {
            match s {
                "ok" | "Ok" | "OK" | "continue" => Some(CommandResult::Continue),
                "break" => Some(CommandResult::Break),
                _ => None,
            }
        }
    }
}

/// Defines all the local commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum Command {
    NoOp,

    Print(print::Print),
    PrintFmt(print::PrintFmt),
    // Sim(sim::SimControl),
    Set(set::Set),
    // SetIntIntAddr(set::SetIntIntAddr),
    Eval(eval::Eval),
    // EvalReg(EvalReg),
    Query(query::Query),

    // Equal(Equal),
    // BiggerThan(BiggerThan),
    // #[cfg(feature = "machine_dynlib")]
    // LibCall(lib::LibCall),
    //
    // Attach(attach::Attach),
    // Detach(detach::Detach),
    Goto(goto::Goto),
    // Jump(jump::Jump),
    //
    // // ext
    Get(get_set::Get),
    //
    // // central ext
    Invoke(invoke::Invoke),
    Spawn(spawn::Spawn),
    //
    // // register
    // RegisterEvent(register::RegisterEvent),
    RegisterEntityPrefab(register::RegisterEntityPrefab),
    RegisterComponent(register::RegisterComponent),
    RegisterTrigger(register::RegisterTrigger),
    RegisterVar(register::RegisterVar),
    // Extend(register::Extend),
    //
    // // register blocks
    Machine(flow::machine::MachineBlock),
    State(flow::state::State),
    Component(flow::component::ComponentBlock),
    //
    // // flow control
    If(flow::ifelse::If),
    Else(flow::ifelse::Else),
    End(flow::end::End),
    Call(flow::call::Call),
    ForIn(flow::forin::ForIn),
    Loop(flow::_loop::Loop),
    Break(flow::_loop::Break),
    Procedure(flow::procedure::Procedure),
    //
    // Range(range::Range),
}

impl Command {
    /// Creates new command struct from a prototype.
    pub fn from_prototype(
        proto: &CommandPrototype,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Command> {
        let cmd_name = match &proto.name {
            Some(c) => c,
            None => return Err(Error::new(location.clone(), ErrorKind::NoCommandPresent)),
        };
        let args = match &proto.arguments {
            Some(a) => a.clone(),
            None => Vec::new(),
        };

        trace!(
            "command from prototype: args: {:?}, location: {:?}",
            args,
            location
        );

        match cmd_name.as_str() {
            "print" => Ok(Command::PrintFmt(print::PrintFmt::from_args(
                args, location,
            )?)),
            "set" => Ok(set::Set::new(args, location)?),
            // // "set" => Ok(get::Get::new(args, location)?),
            "spawn" => Ok(Command::Spawn(spawn::Spawn::new(args, location)?)),
            "invoke" => Ok(Command::Invoke(invoke::Invoke::new(args)?)),
            "query" => query::Query::new(args, location, &commands),
            // "sim" => Ok(sim::SimControl::new(args)?),
            //
            // "extend" => Ok(Command::Extend(register::Extend::new(args, location)?)),
            //
            // // register one-liners
            // "event" => Ok(Command::RegisterEvent(register::RegisterEvent::new(
            //     args, location,
            // )?)),
            "entity" | "prefab" => Ok(Command::RegisterEntityPrefab(
                register::RegisterEntityPrefab::new(args, location)?,
            )),
            "trigger" | "triggered_by" => Ok(Command::RegisterTrigger(
                register::RegisterTrigger::new(args, location)?,
            )),
            "var" => Ok(Command::RegisterVar(register::RegisterVar::new(
                args, location,
            )?)),

            "machine" => Ok(flow::machine::MachineBlock::new(args, location, &commands)?),

            "component" | "comp" => {
                Ok(register::RegisterComponent::new(args, location, &commands)?)
            }
            //
            // // register blocks
            // // "component" | "comp" => Ok(flow::component::ComponentBlock::new(
            // //     args, location, &commands,
            // // )?),
            "state" => Ok(flow::state::State::new(args, location, &commands)?),
            "goto" => Ok(Command::Goto(goto::Goto::new(args)?)),
            //
            // // flow control
            // "jump" => Ok(Command::Jump(jump::Jump::new(args)?)),
            "if" => Ok(Command::If(flow::ifelse::If::new(
                args, location, &commands,
            )?)),
            "else" => Ok(Command::Else(flow::ifelse::Else::new(args)?)),
            "proc" | "procedure" => Ok(Command::Procedure(flow::procedure::Procedure::new(
                args, location, &commands,
            )?)),
            "call" => Ok(Command::Call(flow::call::Call::new(
                args, location, &commands,
            )?)),
            "end" => Ok(Command::End(flow::end::End::new(args)?)),
            "for" => Ok(Command::ForIn(flow::forin::ForIn::new(
                args, location, commands,
            )?)),
            "loop" | "while" => Ok(Command::Loop(flow::_loop::Loop::new(
                args, location, commands,
            )?)),
            "break" => Ok(Command::Break(flow::_loop::Break {})),
            //
            // "range" => Ok(Command::Range(range::Range::new(args)?)),
            //
            "eval" => Ok(eval::Eval::new(args)?),
            //
            // #[cfg(feature = "machine_dynlib")]
            // "lib_call" => Ok(LibCall::new(args)?),
            _ => Err(Error::new(
                location.clone(),
                ErrorKind::UnknownCommand(cmd_name.to_string()),
            )),
        }
    }

    pub async fn execute(
        &self,
        machine: &mut Machine,
        comp_state: &mut StringId,
        call_stack: &mut super::CallStackVec,
        registry: &mut super::Registry,
        comp_name: &CompName,
        ent_id: &EntityId,
        location: &LocationInfo,
        // exec: E,
    ) -> CommandResult {
        // let line = location.line.unwrap();
        trace!("executing command: {:?}", self);
        match self {
            // Command::Sim(cmd) => {
            //     out_res.push(cmd.execute_loc(ent_storage, comp_state, comp_name, location))
            // }
            // Command::Print(cmd) => out_res.push(cmd.execute_loc(ent_storage)),
            Command::PrintFmt(cmd) => cmd.execute(machine).await,
            // Command::Set(cmd) => {
            //     out_res.push(cmd.execute_loc(ent_storage, ent_id, comp_state, comp_name, location))
            // }
            // Command::SetIntIntAddr(cmd) => {
            //     out_res.push(cmd.execute_loc(ent_storage, comp_name, location))
            // }
            //
            // Command::Eval(cmd) => {
            //     out_res.push(cmd.execute_loc(ent_storage, comp_name, registry, location))
            // }
            // // Command::EvalReg(cmd) => out_res.push(cmd.execute_loc(registry)),
            //
            // //Command::Eval(cmd) => out_res.push(cmd.execute_loc(ent_storage)),
            // //Command::Equal(cmd) => out_res.push(cmd.execute_loc(ent_storage)),
            // //Command::BiggerThan(cmd) => out_res.push(cmd.execute_loc(ent_storage)),
            //
            // #[cfg(feature = "machine_dynlib")]
            // Command::LibCall(cmd) => out_res.push(cmd.execute_loc(libs, ent_id, ent_storage)),
            // //Command::Attach(cmd) => out_res.push(cmd.execute_loc(ent, sim_model)),
            // //Command::Detach(cmd) => out_res.push(cmd.execute_loc(ent, sim_model)),
            // Command::Goto(cmd) => out_res.push(cmd.execute_loc(comp_state)),
            // //Command::Jump(cmd) => out_res.push(cmd.execute_loc()),
            //
            // Command::Get(cmd) => cmd.execute().await,
            // Command::RegisterEntityPrefab(cmd) => out_res.extend(cmd.execute_loc()),
            //
            // Command::RegisterComponent(cmd) => out_res.extend(cmd.execute_loc(call_stack)),
            // Command::RegisterVar(cmd) => cmd.execute(call_stack).await,
            // Command::RegisterTrigger(cmd) => out_res.extend(cmd.execute_loc(call_stack)),
            // Command::RegisterEvent(cmd) => out_res.extend(cmd.execute_loc()),
            //
            // Command::Invoke(cmd) => cmd.execute().await,
            // Command::Spawn(cmd) => cmd.execute().await,
            // Command::Call(cmd) => {
            //     out_res.push(cmd.execute_loc(call_stack, line, sim_model, comp_name, location))
            // }
            //
            // Command::Jump(cmd) => out_res.push(cmd.execute_loc()),
            // Command::If(cmd) => {
            //     out_res.push(cmd.execute_loc(call_stack, ent_storage, comp_name, line))
            // }
            // Command::Else(cmd) => out_res.push(cmd.execute_loc(call_stack, ent_storage, location)),
            // Command::ForIn(cmd) => out_res.push(cmd.execute_loc(
            //     call_stack,
            //     registry,
            //     comp_name,
            //     ent_storage,
            //     location,
            // )),
            Command::Loop(cmd) => cmd.execute_loc(call_stack),
            // Command::Break(cmd) => out_res.push(cmd.execute_loc(call_stack, ent_storage, location)),
            //
            // Command::End(cmd) => cmd.execute_loc(call_stack, comp_name, ent_storage, location),
            // Command::Procedure(cmd) => cmd.execute_loc(call_stack, ent_storage, line),
            //
            // Command::State(cmd) => {
            //     out_res.extend(cmd.execute_loc(call_stack, ent_id, comp_name, line))
            // }
            // Command::Component(cmd) => cmd.execute(call_stack, ent_id, comp_name, line).await,
            //
            // Command::Extend(cmd) => out_res.push(cmd.execute_loc()),
            // // Command::Register(cmd) => out_res.extend(cmd.execute_loc(call_stack)),
            // Command::Range(cmd) => out_res.push(cmd.execute_loc(ent_storage, comp_name, location)),
            _ => {
                warn!("unable to execute cmd: {:?}", self);
                CommandResult::Continue
            }
        }
    }
    pub fn run_with_model_context(&self, sim_model: &mut Model) -> CommandResult {
        match self {
            // Command::Register(cmd) => cmd.execute(sim_model),
            // Command::Print(cmd) => cmd.execute_loc(),
            _ => CommandResult::Continue,
        }
    }
}

/// External (non-entity-local) command meant for execution within central
/// authority scope.
///
/// ### Distinction between remote and central-remote
///
/// Distinction is made because of the potentially distributed nature of the
/// simulation. In a distributed setting, there are certain things, like the
/// simulation model, that have to be managed from a central point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CentralRemoteCommand {
    // Sim(sim::SimControl),
    // // Register(register::Register),
    // RegisterComponent(register::RegisterComponent),
    // RegisterTrigger(register::RegisterTrigger),
    RegisterVar(register::RegisterVar),
    // RegisterEntityPrefab(register::RegisterEntityPrefab),
    // RegisterEvent(register::RegisterEvent),
    //
    // Extend(register::Extend),
    // Invoke(invoke::Invoke),
    // Spawn(spawn::Spawn),
    //
    // State(flow::state::State),
    // Component(flow::component::ComponentBlock),
}

impl CentralRemoteCommand {
    /// Executes the command locally, using a reference to the monolithic `Sim`
    /// struct.
    pub fn execute(
        &self,
        // mut sim: &mut SimExec,
        ent_uid: &EntityId,
        comp_uid: &CompName,
    ) -> Result<()> {
        match self {
            // CentralRemoteCommand::Sim(cmd) => cmd.execute_ext(sim),
            //
            // CentralRemoteCommand::RegisterComponent(cmd) => cmd.execute_ext(sim, ent_uid, comp_uid),
            // CentralRemoteCommand::RegisterEntityPrefab(cmd) => cmd.execute_ext(sim),
            //
            // CentralRemoteCommand::RegisterEvent(cmd) => cmd.execute_ext(sim),
            // CentralRemoteCommand::RegisterTrigger(cmd) => cmd.execute_ext(sim, ent_uid, comp_uid),
            // CentralRemoteCommand::RegisterVar(cmd) => cmd.execute_ext(sim, ent_uid, comp_uid),
            //
            // CentralRemoteCommand::Extend(cmd) => cmd.execute_ext(sim, ent_uid),
            // CentralRemoteCommand::Invoke(cmd) => cmd.execute_ext(sim),
            // CentralRemoteCommand::Spawn(cmd) => cmd.execute_ext(sim, ent_uid),
            // // CentralRemoteCommand::Prefab(cmd) => return cmd.execute_ext(sim),
            // CentralRemoteCommand::State(cmd) => cmd.execute_ext(sim),
            // CentralRemoteCommand::Component(cmd) => cmd.execute_ext(sim),
            _ => return Ok(()),
        }
    }

    pub fn execute_distr(
        &self,
        // mut sim: &mut SimExec,
        ent_uid: &EntityId,
        comp_name: &CompName,
    ) -> Result<()> {
        match self {
            // CentralRemoteCommand::Spawn(cmd) => cmd.execute_ext_distr(sim)?,
            // CentralRemoteCommand::RegisterEntityPrefab(cmd) => cmd.execute_ext_distr(sim)?,
            // CentralRemoteCommand::RegisterComponent(cmd) => cmd.execute_ext_distr(sim)?,
            // CentralRemoteCommand::RegisterVar(cmd) => cmd.execute_ext_distr(sim, comp_name)?,
            // CentralRemoteCommand::RegisterTrigger(cmd) => cmd.execute_ext_distr(sim)?,
            // CentralRemoteCommand::RegisterEvent(cmd) => cmd.execute_ext_distr(sim)?,
            // CentralRemoteCommand::State(cmd) => cmd.execute_ext_distr(sim)?,
            // CentralRemoteCommand::Component(cmd) => cmd.execute_ext_distr(sim)?,
            // CentralRemoteCommand::Invoke(cmd) => cmd.execute_ext_distr(sim)?,
            _ => error!("unimplemented: {:?}", self),
        }
        Ok(())
    }
}

/// External command meant for execution on an entity scope
/// that includes operations that don't require access to
/// central authority.
///
/// Can be used to allow message-based communication between
/// entity objects.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum ExtCommand {
    Empty,
    // Get(Get),
    // Set(ExtSet),
    // SetVar(ExtSetVar),
    // RemoteExec(Command),
    // CentralizedExec(CentralExtCommand),
}
impl ExtCommand {
    pub fn execute(
        &self,
        ent_id: &EntityId,
        comp_name: &CompName,
        location: &LocationInfo,
    ) -> Result<()> {
        match self {
            // ExtCommand::Get(cmd) => return cmd.execute_ext(sim, ent_uid, comp_uid, location),
            // ExtCommand::Set(cmd) => return cmd.execute_ext(sim, ent_id, comp_name, location),
            // ExtCommand::SetVar(cmd) => return cmd.execute_ext(sim, exec_ctx),
            _ => return Ok(()),
        }
    }

    pub fn get_type_as_str(&self) -> &str {
        match self {
            // ExtCommand::Set(_) => "set",
            _ => "not implemented",
        }
    }
}
