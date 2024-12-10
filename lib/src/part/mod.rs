mod step;

use fnv::FnvHashMap;

use crate::entity::{BasicEntity, Entity};
use crate::executor::Executor;
use crate::model::{EventModel, PrefabModel};
use crate::signal::Signal;
use crate::{string, EntityId, EntityName, EventName, Result, SimModel, StringId};

/// Distributed simulation node.
///
/// It holds the current clock value, a full copy of the sim model, and
/// a subset of simulation entities.
///
/// Implementation of `SimNode` itself doesn't provide a mechanism for
/// communication between different nodes. It includes custom processing
/// functions that can be used by a higher level leader which will
/// provide it's own connection functionality.
#[derive(Clone)]
pub struct Node {
    /// Current clock according to this node.
    ///
    /// # Synchronisation and clock differences
    ///
    /// If cluster is not running in a strict lockstep, nodes can execute at
    /// different "speeds", causing node clocks to diverge.
    pub clock: usize,
    /// Full copy of the model.
    pub model: SimModel,
    /// Events queued for execution during next step.
    pub event_queue: Vec<StringId>,

    /// Basic entities stored at this node.
    pub entities: FnvHashMap<EntityName, BasicEntity>,
    // /// Map of string entity names to entity ids
    // pub entity_names: FnvHashMap<EntityName, EntityId>,
}

impl Node {
    pub fn from_model(model: SimModel, script_init: bool) -> Result<Self> {
        let mut node = Self::default();
        node.model = model;

        // module script init
        if script_init {
            #[cfg(feature = "machine_script")]
            {
                node.spawn_entity_by_prefab_name(
                    Some(&string::new_truncate("_mods_init")),
                    Some(string::new_truncate("_mods_init")),
                )?;
                node.event_queue.push(string::new_truncate("_scr_init"));
            }
        }

        // add entities
        // let mut esr = vec![];
        // for entity in &sim.model.entity_spawn_requests {
        //     esr.push((
        //         string::new_truncate(&entity.prefab.clone()),
        //         entity.name.clone(),
        //     ));
        // }
        // for (prefab, name) in esr {
        //     sim.spawn_entity_by_prefab_name(some(&prefab), name.map(|s| string::new_truncate(&s)))?;
        // }

        // sim.apply_model_entities();
        // sim.apply_model();

        // setup entities' lua_state
        // sim.setup_lua_state_ent();

        Ok(node)
    }

    pub fn spawn_entity_by_prefab_name(
        &mut self,
        prefab: Option<&StringId>,
        name: Option<StringId>,
    ) -> Result<EntityId> {
        match prefab {
            Some(p) => {
                let prefab = self.model.get_prefab(&p).unwrap().clone();
                self.spawn_entity(Some(&prefab), name)
            }
            None => self.spawn_entity(None, name),
        }
    }

    /// Spawns a new entity based on the given prefab.
    ///
    /// If prefab is `None` then an empty entity is spawned.
    pub fn spawn_entity(
        &mut self,
        prefab: Option<&PrefabModel>,
        name: Option<StringId>,
    ) -> Result<EntityId> {
        trace!(
            "starting spawn_entity: name: {:?}, prefab: {:?}",
            name,
            prefab
        );

        trace!("creating new entity");
        // let now = Instant::now();
        let mut ent = match prefab {
            Some(p) => Entity::from_prefab(p, &self.model)?,
            None => Entity::empty(),
        };
        // trace!(
        //     "creating ent from prefab took: {}ns",
        //     now.elapsed().as_nanos()
        // );
        trace!("creating new entity done");

        // trace!("getting new_uid from pool");
        // let new_uid = self.entity_pool.request_id().unwrap();
        // trace!("getting new_uid from pool done");

        trace!("inserting entity");
        // insert to the name map if applicable
        // if let Some(n) = &name {
        //     if !self.entity_idx.contains_key(n) {
        //         self.entity_idx.insert(n.clone(), new_uid);
        //     } else {
        //         return Err(Error::Other(format!(
        //             "Failed to add entity: entity named \"{}\" already exists",
        //             n,
        //         )));
        //     }
        // }

        // insert to proper machine/non-machine map
        #[cfg(not(feature = "machine"))]
        {
            // no machines here
            // self.entities.insert(new_uid, ent);
        }

        #[cfg(feature = "machine")]
        {
            self.entities.insert(0, ent);
            // // choose
            // if !ent.comp_state.is_empty() && !ent.comp_queue.is_empty() {
            //     self.machined_entities.insert(new_uid, ent);
            // } else {
            //     self.entities.insert(new_uid, ent);
            // }
        }

        trace!("inserting entity done");

        Ok(Default::default())
    }

    pub fn add_event(&mut self, name: EventName) -> Result<()> {
        self.model.events.push(EventModel { id: name.clone() });
        // self.event_queue.push(name);
        Ok(())
    }
}
