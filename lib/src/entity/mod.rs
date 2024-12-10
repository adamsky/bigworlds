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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// All data associated with the entity is stored here
    pub storage: Storage,

    /// List of attached components
    pub components: Vec<CompName>,
    // #[cfg(feature = "machine")]
    // /// List of machine tasks
    // pub machines: Vec<MachineHandle>,
}

// /// Basic building block of the simulation state.
// #[derive(Debug, Clone, Serialize, Deserialize, deepsize::DeepSizeOf)]
// pub struct Entity {
//     /// All data associated with the entity is stored here
//     pub storage: Storage,

//     /// List of attached components
//     pub components: Vec<CompName>,

//     /// Current state of each component-tied state machine
//     #[cfg(feature = "machine")]
//     pub comp_state: FnvHashMap<CompName, StringId>,

//     /// Queue of scheduled component-tied machines for each event
//     #[cfg(feature = "machine")]
//     pub comp_queue: FnvHashMap<EventName, Vec<CompName>>,

//     // Non-serializable aspects of an entity
//     // TODO use cfg_if to include this only if related features are enabled
//     #[serde(skip)]
//     pub insta: EntityNonSer,
// }

impl Entity {
    /// Creates a new entity using the prefab model.
    pub fn from_prefab(prefab: &PrefabModel, model: &Model) -> Result<Entity> {
        trace!("creating new entity from prefab");

        let mut ent = Entity::empty();

        #[cfg(feature = "machine")]
        {
            // ent.comp_queue
            //     .insert(string::new_truncate(crate::DEFAULT_INIT_EVENT), Vec::new());

            // for event in &model.events {
            //     ent.comp_queue
            //         .insert(string::new_truncate(&event.id), Vec::new());
            // }
        }

        for comp in &prefab.components {
            ent.attach(comp.clone(), model)?;
        }

        // TODO setup dyn libs

        Ok(ent)
    }

    /// Creates a new entity from model.
    pub fn from_prefab_name(prefab: EntityName, sim_model: &model::Model) -> Result<Entity> {
        trace!("creating entity from prefab name: {}", prefab);
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
            // #[cfg(feature = "machine")]
            // machines: vec![],
        }
    }

    pub fn attach(&mut self, component: CompName, model: &Model) -> Result<()> {
        let comp_model = model.get_component(&component)?;
        debug!("attaching component: {:?}", comp_model);

        self.components.push(component.clone());

        for var_model in &comp_model.vars {
            self.storage.insert(
                (component.clone(), var_model.name.clone()),
                var_model
                    .default
                    .to_owned()
                    .unwrap_or(var_model.type_.default_value()),
            );
        }

        #[cfg(feature = "machine")]
        {
            // trace!("triggers: {:?}", comp_model.triggers);
            // for trigger in &comp_model.triggers {
            //     let t = string::new_truncate(trigger);
            //     if let Some(q) = self.comp_queue.get_mut(&t) {
            //         trace!("pushing to comp_queue: {}", comp_model.name);
            //         q.push(comp_model.name.clone());
            //     }
            // }
            // // println!("")
            // self.comp_state.insert(
            //     comp_model.name.clone(),
            //     comp_model.logic.start_state.clone(),
            // );
        }

        // debug!("start_state: {}", comp_model.start_state);

        //// ignore components that don't have any states
        //// (besides the built-in 'none' state)
        //// TODO
        // if comp_model.logic.states.len() >= 0 {
        // let comp_uid = (IndexString::from(comp_name).unwrap(),);

        #[cfg(feature = "machine")]
        {
            // if !self.comp_state.contains_key(&component) {
            //     for trigger in &comp_model.triggers {
            //         //                println!("trigger: {}", trigger);
            //         let t = string::new_truncate(trigger);
            //         #[cfg(feature = "machine")]
            //         self.comp_queue.get_mut(&t).unwrap().push(component.clone());
            //     }
            // }
        }

        Ok(())
    }

    pub fn detach(&mut self, comp_name: &CompName, sim_model: &Model) -> Result<()> {
        if let Ok(idx) = self.components.binary_search(comp_name) {
            self.components.remove(idx);
        }
        self.storage
            .remove_comp_vars(comp_name, sim_model.get_component(comp_name)?);

        #[cfg(feature = "machine")]
        {
            // self.comp_state.remove(comp_name);
            // // find and remove references to component from all the queues
            // // for different events
            // for (q, v) in &mut self.comp_queue {
            //     let n = match v.iter().position(|c| c == comp_name) {
            //         Some(p) => p,
            //         None => continue,
            //     };
            //     v.remove(n);
            // }
        }

        Ok(())
    }
}
