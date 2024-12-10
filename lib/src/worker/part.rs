use fnv::FnvHashMap;
use tokio::sync::watch;

use crate::entity::Entity;
use crate::executor::LocalExec;
use crate::model::{EventModel, PrefabModel};
use crate::{EntityId, EntityName, Error, EventName, Model, PrefabName, Result, StringId};

#[cfg(feature = "machine")]
use crate::machine::MachineHandle;

/// Holds data constituting simulation part held by the worker.
#[derive(Clone, Default)]
pub struct Part {
    /// Simulation model kept up to date with leader.
    pub model: Model,

    /// List of entities
    pub entities: FnvHashMap<EntityName, Entity>,

    #[cfg(feature = "machine")]
    pub machines: FnvHashMap<EntityName, Vec<MachineHandle>>,
}

impl Part {
    pub fn from_model(model: Model, mod_script: bool) -> Result<Self> {
        let mut sim = Part::default();
        sim.model = model;

        // module script init
        if mod_script {
            #[cfg(feature = "machine_script")]
            {
                sim.spawn_entity_by_prefab_name(
                    Some(&string::new_truncate("_mods_init")),
                    Some(string::new_truncate("_mods_init")),
                )?;
                sim.event_queue.push(string::new_truncate("_scr_init"));
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
        //     sim.spawn_entity_by_prefab_name(Some(&prefab), name.map(|s| string::new_truncate(&s)))?;
        // }

        // sim.apply_model_entities();
        // sim.apply_model();

        // setup entities' lua_state
        // sim.setup_lua_state_ent();

        Ok(sim)
    }
}

impl Part {
    pub fn spawn_entity_by_prefab_name(
        &mut self,
        prefab: Option<&PrefabName>,
        name: Option<EntityName>,
    ) -> Result<EntityId> {
        match prefab {
            Some(p_name) => {
                let prefab = self
                    .model
                    .prefabs
                    .iter()
                    .find(|p| &p.name == p_name)
                    .ok_or(Error::NoEntityPrefab(p_name.clone()))?
                    .clone();
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
        trace!("spawn_entity: name: {:?}, prefab: {:?}", name, prefab);

        // let now = Instant::now();
        let mut ent = match prefab {
            Some(p) => Entity::from_prefab(p, &self.model)?,
            None => Entity::empty(),
        };

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

        // #[cfg(feature = "machine")]
        // {
        //     // choose
        //     if !ent.comp_state.is_empty() && !ent.comp_queue.is_empty() {
        //         self.machined_entities.insert(new_uid, ent);
        //     } else {
        //         self.entities.insert(new_uid, ent);
        //     }
        // }

        Ok(Default::default())
    }

    pub fn add_event(&mut self, name: EventName) -> Result<()> {
        self.model.events.push(EventModel { id: name.clone() });
        // self.event_queue.push(name);
        Ok(())
    }
    // /// Serialize to a vector of bytes.
    // ///
    // /// # Compression
    // ///
    // /// Optional compression using zstd can be performed.
    // pub fn to_snapshot(&self, name: &str, compress: bool) -> Result<Vec<u8>> {
    //     let mut data = bincode::serialize(self)?;
    //
    //     #[cfg(feature = "zstd")]
    //     {
    //         if compress {
    //             data = lz4::block::compress(&data, None, true)?;
    //         }
    //     }
    //
    //     Ok(())
    // }

    // /// Creates new `Sim` from snapshot, using
    // pub fn load_snapshot(name: &str, compressed: Option<bool>) -> Result<Self> {
    //     let project_path = crate::util::find_project_root(std::env::current_dir()?, 3)?;
    //     let snapshot_path = project_path
    //         .join(engine_core::SNAPSHOTS_DIR_NAME)
    //         .join(name);
    //     let mut file = File::open(snapshot_path)?;
    //     let mut bytes = Vec::new();
    //     file.read_to_end(&mut bytes);
    //     if let Some(compressed) = compressed {
    //         if compressed {
    //             #[cfg(feature = "lz4")]
    //             {
    //                 bytes = lz4::block::decompress(&bytes, None)?;
    //             }
    //         };
    //         let sim = SimLocal::from_snapshot(&mut bytes)?;
    //         Ok(sim)
    //     } else {
    //         // first try reading compressed
    //         #[cfg(feature = "lz4")]
    //         {
    //             // bytes = lz4::block::decompress(&bytes, None)?;
    //             match lz4::block::decompress(&bytes, None) {
    //                 Ok(mut bytes) => {
    //                     let sim = SimLocal::from_snapshot(&mut bytes)?;
    //                     return Ok(sim);
    //                 }
    //                 Err(_) => {
    //                     let sim = SimLocal::from_snapshot(&mut bytes)?;
    //                     return Ok(sim);
    //                 }
    //             }
    //         }
    //         #[cfg(not(feature = "lz4"))]
    //         {
    //             let sim = SimLocal::from_snapshot(&mut bytes)?;
    //             return Ok(sim);
    //         }
    //     }
    // }
}
