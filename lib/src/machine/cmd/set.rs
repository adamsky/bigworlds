use std::str::FromStr;

use crate::address::{Address, LocalAddress, ShortLocalAddress};
use crate::{address, string, CompName, EntityId, EntityName, StringId, Var, VarType};

use crate::entity::{Entity, Storage};
use crate::machine::cmd::get_set::ExtSet;
use crate::machine::cmd::ExtCommand;
use crate::machine::{Error, ErrorKind, Result};

use super::super::LocationInfo;
use super::{Command, CommandResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetIntIntAddr {
    target: (StringId, StringId),
    source: (StringId, StringId),
}
impl SetIntIntAddr {
    pub fn execute_loc(
        &self,
        storage: &mut Storage,
        comp_uid: &CompName,
        location: &LocationInfo,
    ) -> CommandResult {
        *storage
            .get_var_mut(&self.target)
            .unwrap()
            .as_int_mut()
            .unwrap() = *storage.get_var(&self.source).unwrap().as_int().unwrap();
        CommandResult::Continue
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Set {
    target: Target,
    source: Source,
    out: Option<ShortLocalAddress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Target {
    Address(Address),
    LocalAddress(ShortLocalAddress),
}

impl Target {
    pub fn from_str(s: &str, location: &LocationInfo) -> Result<Self> {
        if s.contains(address::SEPARATOR_SYMBOL) {
            let split = s.split(address::SEPARATOR_SYMBOL).collect::<Vec<&str>>();
            if split.len() == 2 {
                return Ok(Target::LocalAddress(ShortLocalAddress {
                    comp: None,
                    var_type: VarType::from_str(split[0])?,
                    var_name: string::new_truncate(split[1]),
                }));
            } else if split.len() == 3 {
                return Ok(Target::LocalAddress(ShortLocalAddress {
                    comp: Some(string::new_truncate(split[0])),
                    var_type: VarType::from_str(split[1])?,
                    var_name: string::new_truncate(split[2]),
                }));
            } else if split.len() == 4 {
                return Ok(Target::Address(Address {
                    entity: string::new_truncate(split[0]),
                    component: string::new_truncate(split[1]),
                    var_type: VarType::from_str(split[2])?,
                    var_name: string::new_truncate(split[3]),
                }));
            } else {
                unimplemented!()
            }
        }
        Err(Error::new(
            location.clone(),
            ErrorKind::Other("failed parsing set.target".to_string()),
        ))
    }

    pub fn var_type(&self) -> VarType {
        match self {
            Target::LocalAddress(a) => a.var_type,
            Target::Address(a) => a.var_type,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Source {
    Address(Address),
    LocalAddress(ShortLocalAddress),
    Value(Var),
}

impl Source {
    pub fn from_str(s: &str, target_type: VarType, location: &LocationInfo) -> Result<Self> {
        if s.contains(address::SEPARATOR_SYMBOL) {
            let split = s.split(address::SEPARATOR_SYMBOL).collect::<Vec<&str>>();
            if split.len() == 2 {
                return Ok(Source::LocalAddress(ShortLocalAddress {
                    comp: None,
                    var_type: VarType::from_str(split[0])?,
                    var_name: string::new_truncate(split[1]),
                }));
            } else if split.len() == 3 {
                return Ok(Source::LocalAddress(ShortLocalAddress {
                    comp: Some(string::new_truncate(split[0])),
                    var_type: VarType::from_str(split[1])?,
                    var_name: string::new_truncate(split[2]),
                }));
            } else if split.len() == 4 {
                return Ok(Source::Address(Address {
                    entity: string::new_truncate(split[0]),
                    component: string::new_truncate(split[1]),
                    var_type: VarType::from_str(split[2])?,
                    var_name: string::new_truncate(split[3]),
                }));
            } else {
                unimplemented!()
            }
        } else {
            let var = match Var::from_str(s, Some(target_type)) {
                Ok(v) => v,
                Err(e) => {
                    return Err(Error::new(
                        location.clone(),
                        ErrorKind::InvalidCommandBody(format!(
                            "can't parse from source into target type: {}",
                            e
                        )),
                    ))
                }
            };
            Ok(Source::Value(var))
        }
    }
}

impl Set {
    pub fn new(args: Vec<String>, location: &LocationInfo) -> Result<Command> {
        let target = Target::from_str(&args[0], location)?;

        let mut source_str = "";
        // is '=' present?
        if args.len() > 1 {
            if args[1] == "=" {
                source_str = &args[2];
            } else {
                source_str = &args[1];
            }
        }

        let source = Source::from_str(source_str, target.var_type(), location)?;

        let mut out = None;
        if let Some((out_sign_pos, _)) = args.iter().enumerate().find(|(_, s)| s.as_str() == "=>") {
            if let Some(out_addr) = args.get(out_sign_pos + 1) {
                out = Some(out_addr.parse()?);
            }
        }

        Ok(Command::Set(Set {
            target,
            source,
            out,
        }))
    }
    // pub async fn execute(
    //     &self,
    //     entity_db: &mut Storage,
    //     ent_uid: &EntityId,
    //     comp_state: &mut StringId,
    //     comp_name: &CompName,
    //     location: &LocationInfo,
    // ) -> CommandResult {
    //     let var_type = self.target.var_type();
    //     let target_addr = match &self.target {
    //         Target::Address(addr) => addr.clone(),
    //         Target::LocalAddress(loc_addr) => Address {
    //             entity: string::new_truncate(&ent_uid.to_string()),
    //             component: loc_addr.comp.clone().unwrap_or(comp_name.clone()),
    //             var_type: loc_addr.var_type,
    //             var_name: loc_addr.var_name.clone(),
    //         },
    //     };
    //
    //     match &self.source {
    //         Source::LocalAddress(loc_addr) => {
    //             // entity_db.set_from_addr(&self.target, &addr)
    //
    //             *entity_db.get_var_mut(&target_addr.storage_index()).unwrap() = entity_db
    //                 .get_var(&loc_addr.storage_index_using(comp_name.clone()))
    //                 .unwrap()
    //                 .clone();
    //         }
    //         Source::Address(addr) => {
    //             return CommandResult::ExecExt(ExtCommand::Set(ExtSet {
    //                 target: target_addr.clone(),
    //                 source: addr.clone(),
    //                 out: Some(
    //                     self.out
    //                         .clone()
    //                         .map(|a| {
    //                             a.into_address(
    //                                 ent_uid.to_string().parse().unwrap(),
    //                                 comp_name.clone(),
    //                             )
    //                         })
    //                         .unwrap()
    //                         .unwrap(),
    //                 ),
    //             }))
    //         }
    //         Source::Value(val) => {
    //             if let Ok(target_var) = entity_db.get_var_mut(&target_addr.storage_index()) {
    //                 *target_var = val.clone();
    //             } else {
    //                 entity_db.insert(target_addr.storage_index(), val.clone());
    //             }
    //         }
    //     }
    //     CommandResult::Continue
    // }
}
