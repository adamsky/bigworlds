use std::collections::HashMap;
use std::future::Future;

use fnv::FnvHashMap;
use futures::future::BoxFuture;
use tokio::sync::watch;

use crate::behavior::BehaviorHandle;
use crate::entity::Entity;
use crate::executor::LocalExec;
use crate::model::{EventModel, PrefabModel};
use crate::{
    behavior, rpc, EntityId, EntityName, Error, EventName, Model, PrefabName, Result, StringId,
};

#[cfg(feature = "machine")]
use crate::machine::MachineHandle;

/// Holds data constituting simulation part held by the worker.
pub struct Part {
    pub entities: FnvHashMap<EntityName, Entity>,

    /// Handles to machine behaviors
    #[cfg(feature = "machine")]
    pub machines: FnvHashMap<EntityName, Vec<MachineHandle>>,

    /// Handles to synced behaviors
    pub behaviors: Vec<BehaviorHandle>,
    /// Broadcast sender for communicating with unsynced behaviors
    pub behavior_broadcast: tokio::sync::broadcast::Sender<rpc::behavior::Request>,

    /// All the currently loaded dynamic libraries
    pub libs: Vec<libloading::Library>,
}

impl Part {
    pub fn new() -> Self {
        Self {
            entities: FnvHashMap::default(),
            machines: FnvHashMap::default(),
            behaviors: vec![],
            behavior_broadcast: tokio::sync::broadcast::channel(20).0,
            libs: vec![],
        }
    }

    /// Initializes the worker `Part` using the given model.
    pub fn from_model(
        model: &Model,
        mod_script: bool,
        worker_exec: LocalExec<
            rpc::worker::Request,
            std::result::Result<rpc::worker::Response, Error>,
        >,
    ) -> Result<Self> {
        let (proc_tx, proc_rx) = tokio::sync::broadcast::channel::<rpc::behavior::Request>(100);

        let mut part = Part {
            entities: HashMap::default(),
            machines: HashMap::default(),
            behaviors: Vec::new(),
            behavior_broadcast: proc_tx.clone(),
            libs: Vec::new(),
        };

        // module script init
        if mod_script {
            #[cfg(feature = "machine_script")]
            {
                part.spawn_entity_by_prefab_name(
                    Some(&string::new_truncate("_mods_init")),
                    Some(string::new_truncate("_mods_init")),
                )?;
                part.event_queue.push(string::new_truncate("_scr_init"));
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

        // initialize dynlib behaviors
        for behavior in &model.behaviors {
            // load the library
            let lib = unsafe { libloading::Library::new(behavior.lib_path.clone()).unwrap() };

            // run the function as a separate `behavior`
            unsafe {
                let function: libloading::Symbol<crate::behavior::BehaviorFnUnsynced> =
                    lib.get(behavior.entry.as_bytes()).unwrap();

                behavior::spawn(
                    *function,
                    proc_tx.subscribe(),
                    worker_exec.clone(),
                    tokio::runtime::Handle::current(),
                )?;
            }

            part.libs.push(lib);
        }

        Ok(part)
    }
}

impl Part {
    // pub fn spawn_entity_by_prefab_name(
    //     &mut self,
    //     prefab: Option<&PrefabName>,
    //     name: Option<EntityName>,
    // ) -> Result<EntityId> {
    //     match prefab {
    //         Some(p_name) => {
    //             let prefab = self
    //                 .model
    //                 .prefabs
    //                 .iter()
    //                 .find(|p| &p.name == p_name)
    //                 .ok_or(Error::NoEntityPrefab(p_name.clone()))?
    //                 .clone();
    //             self.spawn_entity(Some(&prefab), name)
    //         }
    //         None => self.spawn_entity(None, name),
    //     }
    // }

    // /// Spawns a new entity based on the given prefab.
    // ///
    // /// If prefab is `None` then an empty entity is spawned.
    // pub fn spawn_entity(
    //     &mut self,
    //     prefab: Option<&PrefabModel>,
    //     name: Option<StringId>,
    // ) -> Result<EntityId> {
    //     trace!("spawn_entity: name: {:?}, prefab: {:?}", name, prefab);

    //     // let now = Instant::now();
    //     let mut ent = match prefab {
    //         Some(p) => Entity::from_prefab(p, &self.model)?,
    //         None => Entity::empty(),
    //     };

    //     // insert to the name map if applicable
    //     // if let Some(n) = &name {
    //     //     if !self.entity_idx.contains_key(n) {
    //     //         self.entity_idx.insert(n.clone(), new_uid);
    //     //     } else {
    //     //         return Err(Error::Other(format!(
    //     //             "Failed to add entity: entity named \"{}\" already exists",
    //     //             n,
    //     //         )));
    //     //     }
    //     // }

    //     // insert to proper machine/non-machine map
    //     #[cfg(not(feature = "machine"))]
    //     {
    //         // no machines here
    //         // self.entities.insert(new_uid, ent);
    //     }

    //     // #[cfg(feature = "machine")]
    //     // {
    //     //     // choose
    //     //     if !ent.comp_state.is_empty() && !ent.comp_queue.is_empty() {
    //     //         self.machined_entities.insert(new_uid, ent);
    //     //     } else {
    //     //         self.entities.insert(new_uid, ent);
    //     //     }
    //     // }

    //     Ok(Default::default())
    // }

    // pub fn add_event(&mut self, name: EventName) -> Result<()> {
    //     self.model.events.push(EventModel { id: name.clone() });
    //     // self.event_queue.push(name);
    //     Ok(())
    // }

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
