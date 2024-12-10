//! Step processing functions.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use futures::stream::FuturesUnordered;
use futures::StreamExt;

use crate::{string, EntityId, EntityName, StringId};

use crate::entity::Entity;
use crate::error::Error;
use crate::SimModel;

#[cfg(feature = "machine")]
use crate::machine::{cmd::CentralRemoteCommand, cmd::ExtCommand, exec, ExecutionContext};
#[cfg(feature = "machine")]
use rayon::prelude::*;

#[cfg(feature = "machine_dynlib")]
use crate::machine::Libraries;
#[cfg(feature = "machine_dynlib")]
use libloading::Library;

use crate::executor::Executor;
use crate::model::ComponentModel;
use crate::signal::Signal;

use super::Node;

impl Node {
    /// Performs single simulation step.
    pub async fn step<
        E: Executor<Signal, crate::Result<Signal>> + Clone + Send + Sync + 'static,
    >(
        &mut self,
        exec: E,
    ) -> Result<(), Error> {
        // clone event queue into a local variable
        let mut event_queue = self.event_queue.clone();

        let arrstr_step = string::new_truncate("step");
        if !event_queue.contains(&arrstr_step) {
            event_queue.push(arrstr_step.clone());
        }
        self.event_queue.clear();

        #[cfg(feature = "machine")]
        {
            let model = &self.model;

            #[cfg(feature = "machine_dynlib")]
            let libs = &self.libs;

            let tasks: FuturesUnordered<_> = self
                .entities
                .drain()
                .map(|(entity_id, mut entity)| {
                    let mut components = Vec::new();
                    for event in &event_queue {
                        let comps = entity.comp_queue.get(event).unwrap();
                        let comps: Vec<ComponentModel> = comps
                            .iter()
                            .map(|c| model.get_component(c).unwrap().clone())
                            .collect();
                        components.extend(comps);
                    }
                    let exec = exec.clone();

                    tokio::spawn(async move {
                        step_entity(entity_id, &mut entity, components, exec).await;
                        (entity_id, entity)
                    })
                })
                .collect();

            let results: Vec<_> = tasks.collect().await;
            for result in results {
                let (id, entity) = result.unwrap();
                *self.entities.get_mut(&id).unwrap() = entity;
            }
            // self.entities = results.into_iter().map(|r| r.unwrap()).collect();
        }

        self.clock += 1;

        if !self.event_queue.contains(&arrstr_step) {
            self.event_queue.push(arrstr_step);
        }

        Ok(())
    }
}

#[cfg(feature = "machine")]
pub(crate) async fn step_entity<E: Executor<Signal, crate::Result<Signal>> + Clone>(
    ent_uid: EntityId,
    mut entity: &mut Entity,
    comps: Vec<ComponentModel>,
    executor: E,
) -> Result<(), Error> {
    // trace!(
    //     "step_entity: entity.comp_queue: {:?}",
    //     entity.comp_queue
    // );
    for comp_model in comps {
        if let Some(comp_state) = entity.comp_state.get_mut(&comp_model.name) {
            // trace!("comp_state: {}", comp_state);
            // let comp_curr_state = &comp.current_state;
            if comp_state.as_str() == "idle" {
                continue;
            }

            trace!("comp_model: {:?}", comp_model);
            let (start, end) = match comp_model.logic.states.get(comp_state) {
                Some((s, e)) => (Some(*s), Some(*e)),
                None => continue,
            };

            crate::machine::exec::execute_loc(
                &comp_model.logic.commands,
                &comp_model.logic.cmd_location_map,
                &mut entity.storage,
                &mut entity.insta,
                comp_state,
                &ent_uid,
                &comp_model.name,
                start,
                end,
                executor.clone(),
            )
            .await?;
        }
    }
    // for (comp_uid, mut comp) in &mut entity.components.map {
    // for comp_uid in entity.components.map.keys()
    //     .map(|c| *c).collect::<Vec<(ShortString,
    // ShortString)>>().iter() {

    // debug!("inside entity: {:?}, processing comp from queue, id:
    // {:?}", &ent_uid, &comp_uid); let mut comp = match
    // entity.components.get_mut(&comp_uid) {     Some(comp) =>
    // comp,     None => {
    //         let (comp_type, comp_id) = &comp_uid;
    //         debug!("failed getting component: {}/{} (perhaps it was
    // recently detached?)",                comp_type.as_str(),
    // comp_id.as_str());         continue;
    //     }
    // };

    // let (mut start, mut end) = (None, None);
    // if !comp_model.logic.states.is_empty() {
    //     let (s, e) =
    //         match
    // &comp_model.logic.states.get(comp_curr_state.as_str())
    // {             Some(se) => se,
    //             None => continue,
    //         };
    //     start = Some(*s);
    //     end = Some(*e);
    // }
    // }

    // remove selected components from ent event queues
    // for r in to_remove_from_ent_queue {
    //     let (n, _) = entity
    //         .components
    //         .queue
    //         .get(event)
    //         .unwrap()
    //         .iter()
    //         .enumerate()
    //         .find(|(n, puid)| **puid == r)
    //         .unwrap();
    //     entity.components.queue.get_mut(event).unwrap().remove(n);
    // }

    Ok(())
}
