use std::borrow::Borrow;
use std::collections::HashMap;
use std::str::FromStr;

use crate::entity::{Entity, Storage};
use crate::machine::error::{Error, ErrorKind};
use crate::machine::{ExecutionContext, LocationInfo, Result};
use crate::model::Model;
use crate::{model, Address, CompName, EntityId, EntityName, StringId, Var, VarType};

use super::{Command, CommandResult, ExtCommand};

/// Sets var at local address on entity to a value of a var
/// at external address on another entity. Can only be
/// executed during `pre` phase, as it accesses
/// data from another entity (it's an `ExtCommand`).
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct Get {
    pub target: Address,
    pub source: Address,
}

impl Get {
    // pub fn from_str(args_str: &str) -> MachineResult<Self> {
    //     let split: Vec<&str> = args_str.split(" ").collect();
    //     // only accepted argument is an address?
    //     if split.len() != 2 {
    //         return Err(MachineError::new(
    //             *location,
    //             MachineErrorKind::Initialization("expected 2
    // arguments".to_string()),         ));
    //     }
    //
    //     if !split[0].contains("/") {
    //         return Err(MachineError::Initialization(
    //             "first argument invalid".to_string(),
    //         ));
    //     }
    //     let loc_addr = Address::from_str_with_context(split[0].trim(), None,
    // None).unwrap();     let ext_addr =
    // Address::from_str(split[1].trim()).unwrap();
    //
    //     Ok(Get {
    //         target: loc_addr,
    //         source: ext_addr,
    //     })
    // }
    pub async fn execute(&self) -> CommandResult {
        // CommandResult::ExecExt(ExtCommand::Get(self.clone()))
        CommandResult::Continue
    }

    // pub fn execute_ext(&self, model: &mut SimModel, exec_ctx: &ExecutionContext) -> Result<()> {
    //     let ent_uid = exec_ctx.ent;
    //     // println!("{:?}, {:?}", self.source.get_ent_type,
    //     // self.source.get_ent_id)
    //     let ext_ent = match sim.get_entity_by_name(&self.source.entity)
    //         // .entities
    //         // .get(&(self.source.get_ent_type(), self.source.get_ent_id()))
    //     {
    //         Ok(e) => e,
    //         Err(_) => {
    //             debug!(
    //                 "executing pre query failed: entity not found: {}",
    //                 self.source.to_string()
    //             );
    //             return Ok(());
    //         }
    //     };
    //     let ext_var = ext_ent
    //         .storage
    //         .get_var(&self.source.storage_index())
    //         .unwrap()
    //         .clone();
    //     let loc_ent = match sim.get_entity_mut(&ent_uid) {
    //         Ok(e) => e,
    //         Err(_) => {
    //             debug!("failed");
    //             return Ok(());
    //         }
    //     };
    //     *loc_ent
    //         .storage
    //         .get_var_mut(&self.target.storage_index())
    //         .unwrap() = ext_var;
    //     return Ok(());
    // }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct Set {
    pub var1: Address,
    pub var2: Option<Address>,
    pub val: Option<StringId>,
}

impl FromStr for Set {
    type Err = super::Error;

    fn from_str(args_str: &str) -> Result<Self> {
        let split: Vec<&str> = args_str.split(" ").collect();
        // only accepted argument is an address?
        if split.len() != 2 {
            // return Err("expected 2 arguments".to_string());
            unimplemented!()
        }

        if !split[0].contains("/") {
            // return Err("first argument invalid".to_string());
            unimplemented!()
        }

        //todo
        unimplemented!()
        // let var1 = Address::from_str_with_context(split[0].trim(), None,
        // None).unwrap();
        //
        // let mut val = None;
        // let mut var2 = None;
        // if split[1].contains("/") {
        //     let ref2 = Address::from_str_with_context(split[1].trim(), None,
        // None).unwrap(); } else {
        //     //            val = Some(split[1].to_string());
        //     val = Some(StringId::from(split[1]).unwrap());
        // }
        //
        // Ok(Set { var1, var2, val })
    }
}
impl Set {
    pub fn execute_loc(&self, es: &mut Storage) -> CommandResult {
        if let Some(u) = &self.var2 {
            es.set_from_addr(&self.var1, &self.var2.clone().unwrap());
        } else if let Some(v) = &self.val {
            es.set_from_str(&self.var1, v.as_str());
        }
        CommandResult::Continue
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtSetVar {
    pub target: Address,
    pub source: Var,
}
impl ExtSetVar {
    pub fn execute_ext(&self, exec_ctx: &ExecutionContext) -> Result<()> {
        unimplemented!();
        // let ext_ent = match sim
        //.entities
        //.get_mut(&(self.target.get_ent_type(), self.target.get_ent_id()))
        //{
        // Some(e) => e,
        // None => {
        // debug!(
        //"execute ext failed: entity not found: {}",
        // self.target.to_string()
        //);
        // return Ok(());
        //};
        // ext_ent
        //.storage
        //.set_from_var(&self.target.as_loc(), self.source.
        //.set_from_var(&self.target.as_loc(), clone());
        // return Ok(());
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct ExtSet {
    pub target: Address,
    pub source: Address,
    pub out: Option<Address>,
}
impl ExtSet {
    // pub fn execute_ext(
    //     &self,
    //     sim: &mut SimHandle,
    //     ent_id: &EntityId,
    //     comp_name: &CompName,
    //     location: &LocationInfo,
    // ) -> Result<()> {
    //     let var = match sim.get_var(&self.source) {
    //         Ok(v) => v.clone(),
    //         Err(e) => {
    //             if let Some(out) = &self.out {
    //                 *sim.get_var_mut(&out)? = Var::Bool(false).coerce(out.var_type)?;
    //             }
    //             return Err(Error::new(
    //                 location.clone(),
    //                 ErrorKind::CoreError(e.to_string()),
    //             ));
    //         }
    //     };
    //     match sim.get_var_mut(&self.target) {
    //         Ok(target) => *target = var,
    //         Err(e) => {
    //             if let Some(out) = &self.out {
    //                 *sim.get_var_mut(&out)? = Var::Bool(false).coerce(out.var_type)?;
    //             }
    //             return Err(Error::new(
    //                 location.clone(),
    //                 ErrorKind::CoreError(e.to_string()),
    //             ));
    //         }
    //     }
    //
    //     if let Some(out) = &self.out {
    //         *sim.get_var_mut(&out)? = Var::Bool(true).coerce(out.var_type)?;
    //     }
    //
    //     // sim.get_var(&self.source)
    //     //     .map_err(|e| Error::new(*location, ErrorKind::CoreError(e.to_string())))?
    //     //     .clone();
    //
    //     // *sim.get_var_mut(&self.target)
    //     //     .map_err(|e| Error::new(*location, ErrorKind::CoreError(e.to_string())))?
    //     // = sim     .get_var(&self.source)
    //     //     .map_err(|e| Error::new(*location, ErrorKind::CoreError(e.to_string())))?
    //     //     .clone();
    //     // if let Some(out) = self.out {
    //     //     *sim.get_var_mut(&out)? = sim.get_var(&out)?.coerce(out.var_type)?;
    //     // }
    //     return Ok(());
    // }
}
