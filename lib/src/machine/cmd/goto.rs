use crate::machine::cmd::CommandResult;
use crate::machine::Result;
use crate::{string, StringId};

/// Goto
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct Goto {
    pub target_state: StringId,
}

impl Goto {
    pub fn new(args: Vec<String>) -> Result<Self> {
        Ok(Goto {
            target_state: string::new_truncate(&args[0]),
        })
    }
    pub fn execute_loc(&self, comp_state: &mut StringId) -> CommandResult {
        *comp_state = self.target_state.clone();
        CommandResult::Continue
        // CommandResult::GoToState(self.target_state.clone())
    }
}
