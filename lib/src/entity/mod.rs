//! Entity structure related definitions.

mod storage;

pub use self::storage::Storage;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use fnv::FnvHashMap;

use crate::error::{Error, Result};
use crate::model::{self, Model, PrefabModel};
use crate::{string, CompName, EntityName, EventName, StringId};

#[cfg(feature = "machine_dynlib")]
use libloading::Library;

pub use storage::StorageIndex;

/// Core building block of a simulation.
///
/// Each entity holds variables organized on the basis of attached components.
///
/// Entities are defined in a way that makes them easy to send between workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// All data associated with the entity.
    pub storage: Storage,

    /// List of attached components.
    ///
    /// This list is updated automatically based on "schema-full" interactions
    /// with the simulation.
    ///
    /// NOTE: It's possible to insert data into an entity bypassing this list,
    /// this however has consequences for visibility in cases like querying
    /// and attaching behaviors based on given entity's component makeup.
    pub components: Vec<CompName>,
}

impl Entity {
    /// Creates a new entity using a prefab model.
    pub fn from_prefab(prefab: &PrefabModel, model: &Model) -> Result<Entity> {
        let mut ent = Entity::empty();

        for comp in &prefab.components {
            ent.attach(comp.clone(), model)?;
        }

        Ok(ent)
    }

    /// Creates a new entity from model.
    pub fn from_prefab_name(prefab: EntityName, sim_model: &model::Model) -> Result<Entity> {
        let ent_model = sim_model
            .get_prefab(&prefab)
            .ok_or(Error::NoEntityPrefab(prefab))?;
        Entity::from_prefab(ent_model, sim_model)
    }

    /// Creates a new empty entity.
    pub fn empty() -> Self {
        Entity {
            storage: Storage::default(),
            components: vec![],
        }
    }

    /// Attaches a component to the entity, based on the provided model.
    pub fn attach(&mut self, component: CompName, model: &Model) -> Result<()> {
        let comp_model = model.get_component(&component)?;

        // Mark the entity as having the component attached
        self.components.push(component.clone());

        // Initialize component data on the storage as seen in the model
        // definition
        for var_model in &comp_model.vars {
            self.storage.insert(
                (component.clone(), var_model.name.clone()),
                var_model
                    .default
                    .to_owned()
                    .unwrap_or(var_model.type_.default_value()),
            );
        }

        Ok(())
    }

    /// Detaches a component from the entity.
    pub fn detach(&mut self, comp_name: &CompName, sim_model: &Model) -> Result<()> {
        // Mark the entity as not having the component attached anymore
        if let Ok(idx) = self.components.binary_search(comp_name) {
            self.components.remove(idx);
        }

        // Remove any data that is associated with the component
        // TODO: consider using the model-constrained version of this function,
        // maybe provide another variant of the `detach` function for that.
        self.storage.remove_comp_vars(comp_name);

        Ok(())
    }
}
