use arrayvec::ArrayVec;

use crate::{CompName, Int, Var, VarType};

use crate::entity::{Entity, Storage};
use crate::model::Model;

use super::super::super::error::Error;
use super::super::super::{CallInfo, CallStackVec, LocationInfo, ProcedureCallInfo};
use super::super::CommandResult;
use super::forin::ForIn;

pub const COMMAND_NAMES: [&'static str; 1] = ["end"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct End {}

impl End {
    pub fn new(args: Vec<String>) -> Result<End, Error> {
        Ok(End {})
    }
    pub fn execute_loc(
        &self,
        call_stack: &mut CallStackVec,
        comp_uid: &CompName,
        ent_storage: &mut Storage,
        location: &LocationInfo,
    ) -> CommandResult {
        let mut do_pop = false;
        // make sure the stack is not empty
        let clen = call_stack.len();
        if clen <= 0 {
            return CommandResult::Continue;
        }
        // peek the stack and process flow control aspects accordingly
        match call_stack.last_mut().unwrap() {
            // CallInfo::Procedure()
            CallInfo::ForIn(ref mut fici) => {
                // forin that's still not finished iterating should not be popped off
                if fici.iteration < fici.target_len {
                    if let Some(target) = &fici.target {
                        if let Some(source_variable) = &fici.variable {
                            // update the iterator variable
                            ForIn::update_variable(
                                source_variable,
                                // &fici.variable_type,
                                target,
                                fici.iteration,
                                ent_storage,
                            );
                        } else {
                            if let Ok(int_var) = ent_storage
                                .get_var_mut(&target.storage_index())
                                .unwrap()
                                .as_int_mut()
                            {
                                *int_var = fici.iteration as Int;
                            }
                        }
                    }

                    fici.iteration = fici.iteration + 1;
                    return CommandResult::JumpToLine(fici.start + 1);
                } else {
                    do_pop = true;
                }
            }
            CallInfo::IfElse(ieci) => {}
            CallInfo::Loop(ci) => return CommandResult::JumpToLine(ci.start + 1),
            _ => do_pop = true,
        };

        // here we actually pop the stack and process contents as needed
        if do_pop {
            let ci = match call_stack.pop() {
                Some(c) => c,
                //None => return CommandResult::Error()
                None => panic!(),
            };
            match ci {
                CallInfo::Procedure(pci) => {
                    if pci.end_line == location.line.unwrap() {
                        return CommandResult::JumpToLine(pci.call_line + 1);
                    }
                }
                _ => (),
            };
        }
        CommandResult::Continue
    }
}
