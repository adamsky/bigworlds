use crate::machine::cmd::{CentralRemoteCommand, CommandResult};
use crate::machine::Result;
use crate::{string, SimHandle, StringId};

/// Invoke
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoke {
    pub events: Vec<StringId>,
}
impl Invoke {
    pub fn new(args: Vec<String>) -> Result<Self> {
        let mut events = Vec::new();
        for arg in &args {
            events.push(string::new_truncate(arg));
        }
        Ok(Invoke { events })
    }
}
impl Invoke {
    pub async fn execute(&self) -> CommandResult {
        // return CommandResult::ExecCentralExt(CentralRemoteCommand::Invoke(self.clone()));
        unimplemented!()
    }
    // pub fn execute_ext(&self, sim: &mut SimHandle) -> Result<()> {
    //     for event in &self.events {
    //         if !sim.event_queue.contains(event) {
    //             sim.event_queue.push(event.to_owned());
    //         }
    //     }
    //     Ok(())
    // }
    // pub fn execute_ext_distr(&self, sim: &mut SimHandle) -> Result<()> {
    //     for event in &self.events {
    //         if !sim.event_queue.contains(event) {
    //             sim.event_queue.push(event.to_owned());
    //         }
    //     }
    //     Ok(())
    // }
}
