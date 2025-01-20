use std::collections::HashMap;

use fnv::FnvHashMap;

use crate::address::{Address, LocalAddress};
use crate::error::{Error, Result};
use crate::{model, string, CompName, StringId, Var, VarName, VarType};

pub type StorageIndex = (CompName, VarName);

/// Entity's data storage structure.
// TODO: benchmark performance of the alternative storage layouts
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Storage {
    pub map: FnvHashMap<StorageIndex, Var>,
}

impl Storage {
    pub fn get_var(&self, idx: &StorageIndex) -> Result<&Var> {
        self.map
            .get(&idx)
            .ok_or(Error::FailedGettingVarFromEntityStorage(idx.clone()))
    }

    pub fn get_var_mut(&mut self, idx: &StorageIndex) -> Result<&mut Var> {
        self.map
            .get_mut(&idx)
            .ok_or(Error::FailedGettingVarFromEntityStorage(idx.clone()))
    }

    pub fn get_all_coerce_to_string(&self) -> HashMap<String, String> {
        let mut out_map = HashMap::new();
        for (index, var) in &self.map {
            // println!("{:?}, {:?}", index, var);
            let (comp_name, var_name) = index;
            out_map.insert(
                format!("{}:{}:{}", comp_name, var.get_type().to_str(), var_name),
                var.to_string(),
            );
        }
        out_map
    }

    pub fn insert(&mut self, idx: (CompName, VarName), var: Var) {
        self.map.insert(idx, var);
    }

    pub fn set_from_str(&mut self, target: &Address, val: &str) {
        unimplemented!();
    }
    pub fn set_from_addr(&mut self, target: &Address, source: &Address) {
        unimplemented!();
    }
    pub fn set_from_var(&mut self, target: &Address, var: &Var) -> Result<()> {
        let target = self.get_var_mut(&(target.component.clone(), target.var_name.clone()))?;
        *target = var.clone();
        Ok(())
    }

    /// Removes all component-associated vars based on given component name.
    ///
    /// NOTE: this operation is expensive as it needs to iterate over all
    /// storage entries. This is due to how storage indexes it's entries with
    /// a (comp_name, var_name) pair. If it's acceptable to use var names
    /// defined for the component in the model, use [`remove_comp_vars_model`]
    /// instead.
    pub fn remove_comp_vars(&mut self, comp_name: &CompName) {
        let mut to_remove = vec![];
        for ((_comp_name, var_name), _) in &self.map {
            if _comp_name == comp_name {
                to_remove.push((_comp_name.clone(), var_name.clone()));
            }
        }
        for r in to_remove {
            self.map.remove(&r);
        }
    }

    /// Removes all component-associated vars that are known to the model
    /// based on the given component name.
    pub fn remove_comp_vars_model(&mut self, comp_name: &CompName, comp_model: &model::Component) {
        for var_model in &comp_model.vars {
            self.map
                .remove(&(comp_name.clone(), var_model.name.clone()));
        }
    }
}
