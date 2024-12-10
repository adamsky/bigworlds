use crate::{string, CompName, ShortString, StringId};

use crate::entity::{Entity, Storage};
use crate::model::{self, Model};

use crate::machine::cmd::{CommandPrototype, CommandResult, LocationInfo};
use crate::machine::error::{Error, ErrorKind};
use crate::machine::{
    CallInfo, CallStackVec, IfElseCallInfo, IfElseMetaData, ProcedureCallInfo, Registry,
};

/// Call a procedure by name.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Call {
    /// Name of the procedure to call
    pub proc_name: ShortString,
}

impl Call {
    pub fn new(
        args: Vec<String>,
        location: &LocationInfo,
        commands: &Vec<CommandPrototype>,
    ) -> Result<Call, Error> {
        Ok(Call {
            proc_name: args[0].parse().unwrap(),
        })
    }

    pub fn execute_loc(
        &self,
        call_stack: &mut CallStackVec,
        line: usize,
        sim_model: &Model,
        comp_uid: &CompName,
        location: &LocationInfo,
    ) -> CommandResult {
        // get the model of the currently executed component
        let comp_model = sim_model.get_component(comp_uid).unwrap();

        // find the procedure by name and get it's start and end lines
        // let (start_line, end_line) = match comp_model.logic.procedures.get(&self.proc_name) {
        //     Some(se) => se,
        //     None => {
        //         return CommandResult::Err(Error::new(
        //             location.clone(),
        //             ErrorKind::InvalidCommandBody(format!(
        //             "call failed: procedure with the name `{}` doesn't exist in the current scope",
        //             &self.proc_name
        //         )),
        //         ))
        //     }
        // };

        // push the call to the call stack
        // call_stack.push(CallInfo::Procedure(ProcedureCallInfo {
        //     call_line: line,
        //     start_line: *start_line,
        //     end_line: *end_line,
        // }));

        // continue execution at the beginning of the called procedure
        // CommandResult::JumpToLine(start_line + 1)
        unimplemented!()
    }
}
