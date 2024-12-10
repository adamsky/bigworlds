use std::str::FromStr;

use crate::address::ShortLocalAddress;
use crate::machine::cmd::{CentralRemoteCommand, CommandResult};
use crate::machine::{Error, ErrorKind, LocationInfo, Result};
use crate::{string, EntityId, StringId};

/// Spawn
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct Spawn {
    pub prefab: Option<StringId>,
    pub spawn_id: Option<StringId>,
    pub out: Option<ShortLocalAddress>,
}
impl Spawn {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Self> {
        let matches = getopts::Options::new()
            .optopt("o", "out", "", "")
            .parse(&args)
            .map_err(|e| Error::new(location.clone(), ErrorKind::ParseError(e.to_string())))?;

        let out = matches
            .opt_str("out")
            .map(|s| ShortLocalAddress::from_str(&s))
            .transpose()?;

        if matches.free.len() == 0 {
            Ok(Self {
                prefab: None,
                spawn_id: None,
                out,
            })
        } else if matches.free.len() == 1 {
            Ok(Self {
                prefab: Some(string::new_truncate(&args[0])),
                spawn_id: None,
                out,
            })
        } else if matches.free.len() == 2 {
            Ok(Self {
                prefab: Some(string::new_truncate(&args[0])),
                spawn_id: Some(string::new_truncate(&args[1])),
                out,
            })
        } else {
            return Err(Error::new(
                location.clone(),
                ErrorKind::InvalidCommandBody("can't accept more than 2 arguments".to_string()),
            ));
        }
    }

    pub async fn execute(&self) -> CommandResult {
        // CommandResult::ExecCentralExt(CentralRemoteCommand::Spawn(self.clone()))
        unimplemented!()
    }

    // pub fn execute_ext(&self, sim: &mut SimHandle, ent_uid: &EntityId) -> Result<()> {
    //     sim.spawn_entity_by_prefab_name(self.prefab.as_ref(), self.spawn_id.clone())?;
    //     Ok(())
    // }
    // pub fn execute_ext_distr(&self, sim: &mut SimHandle) -> Result<()> {
    //     // central.spawn_entity(
    //     //     self.prefab.clone(),
    //     //     self.spawn_id.clone(),
    //     //     Some(DistributionPolicy::Random),
    //     // )?;
    //     Ok(())
    // }
}
