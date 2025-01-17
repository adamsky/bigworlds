//! Data query system.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use fnv::FnvHashMap;

use crate::address::LocalAddress;
// use crate::query::{AddressedTypedMap, Description, Filter, Layout, Map,
// Query, QueryProduct};
use crate::entity::Entity;
use crate::time::Instant;
use crate::{
    string, Address, CompName, EntityId, EntityName, EventName, Float, Int, Result, StringId, Var,
    VarName, VarType,
};

/// Alternative query structure compatible with environments that don't
/// support native query's variant enum layout.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Query {
    pub trigger: Trigger,
    pub description: Description,
    pub layout: Layout,
    pub filters: Vec<Filter>,
    pub mappings: Vec<Map>,
    pub scope: Scope,
}

impl Query {
    pub fn description(mut self, description: Description) -> Self {
        self.description = description;
        self
    }

    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn map(mut self, map: Map) -> Self {
        self.mappings.push(map);
        self
    }
}

/// Uniform query product type.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum QueryProduct {
    /// Address
    NativeAddressedVar(FnvHashMap<(EntityId, CompName, VarName), Var>),
    AddressedVar(FnvHashMap<Address, Var>),
    AddressedTyped(AddressedTypedMap),
    OrderedVar(u32, Vec<Var>),
    Var(Vec<Var>),
    Empty,
}

impl QueryProduct {
    pub fn merge(&mut self, other: QueryProduct) -> Result<()> {
        match self {
            QueryProduct::NativeAddressedVar(hash_map) => todo!(),
            QueryProduct::AddressedVar(hash_map) => todo!(),
            QueryProduct::AddressedTyped(addressed_typed_map) => todo!(),
            QueryProduct::OrderedVar(_, vec) => todo!(),
            QueryProduct::Var(ref mut vec) => match other {
                QueryProduct::NativeAddressedVar(hash_map) => todo!(),
                QueryProduct::AddressedVar(hash_map) => todo!(),
                QueryProduct::AddressedTyped(addressed_typed_map) => todo!(),
                QueryProduct::OrderedVar(_, vec) => todo!(),
                QueryProduct::Var(_vec) => {
                    vec.extend(_vec);
                }
                QueryProduct::Empty => todo!(),
            },
            QueryProduct::Empty => *self = other,
        };

        Ok(())
    }

    pub fn to_vec(self) -> Vec<Var> {
        match self {
            QueryProduct::Var(vec) => vec,
            _ => unimplemented!(),
        }
    }

    pub fn to_map(self) -> FnvHashMap<Address, Var> {
        match self {
            QueryProduct::AddressedVar(map) => map,
            _ => unimplemented!(),
        }
    }
}

impl TryFrom<crate::rpc::worker::Response> for QueryProduct {
    type Error = crate::Error;

    fn try_from(response: crate::rpc::worker::Response) -> std::result::Result<Self, Self::Error> {
        use crate::rpc::worker::Response;
        match response {
            Response::Query(product) => Ok(product),
            _ => Err(crate::Error::UnexpectedResponse(format!("{}", response))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct GlobAddress {
    pub entity: String,
    pub component: String,
    pub var_type: String,
    pub var_id: String,
}

#[derive(Default, Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct AddressedTypedMap {
    pub strings: FnvHashMap<Address, String>,
    pub ints: FnvHashMap<Address, Int>,
    pub floats: FnvHashMap<Address, Float>,
    pub bools: FnvHashMap<Address, bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub enum Trigger {
    /// Immediate, one-time data transfer
    #[default]
    Immediate,
    /// Trigger each time specific event(s) is fired
    SubscribeEvent(EventName),
    /// Trigger each time certain data point is mutated
    SubscribeMutation(Address),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Filter {
    /// Select entities that have all the specified components
    AllComponents(Vec<CompName>),
    /// Select entities that have one or more of specified components
    SomeComponents(Vec<CompName>),
    /// Select entities that have the specified component
    Component(CompName),
    /// Select entities that match any of the provided names
    Name(Vec<EntityName>),
    /// Filter by entity id
    Id(Vec<EntityId>),
    /// Filter by some variable being in specified range
    VarRange(LocalAddress, Var, Var),
    /// Filter by some variable being in specified range
    AttrRange(StringId, Var, Var),
    /// Filter by entity distance to some point, matching on the position
    /// component (x, y and z coordinates, then x,y and z max distance)
    // TODO use single address to vector3 value
    Distance(Address, Address, Address, Float, Float, Float),
    /// Filter by entity distance to any of multiple points.
    DistanceMultiPoint(Vec<(Address, Address, Address, Float, Float, Float)>),
    /// Select entities based on where they are currently stored
    Node(NodeFilter),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum NodeFilter {
    Local(Option<u32>),
    Remote(Option<u32>),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Map {
    /// Map all the data stored on selected entities
    All,
    /// Select data based on address string matching
    SelectAddr(Vec<GlobAddress>),
    /// Select data bound to selected components
    Components(Vec<CompName>),
    Var(VarType, VarName),
    VarName(VarName),
    VarType(VarType),
}

impl Map {
    pub fn components(components: Vec<&str>) -> Self {
        let mut c = Vec::<StringId>::new();
        for comp in components.into_iter() {
            c.push(string::new_truncate(comp));
        }
        Map::Components(c)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub enum Description {
    NativeDescribed,
    /// Self-described values, each with attached address
    Addressed,
    StringAddressed,
    // CustomAddressed,
    /// Values ordered based on an order table
    Ordered,
    #[default]
    None,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub enum Layout {
    /// Use the internal value representation type built on Rust's enum
    #[default]
    Var,
    /// Use a separate map/list for each variable type
    Typed,
    // TypedSubset(Vec<VarType>),
}

/// Defines how widely should the search be spread across the cluster.
/// Effectively it's a way to potentially limit the number of workers a query
/// should be performed on.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub enum Scope {
    /// Broadcast the query across the whole cluster
    #[default]
    Global,
    /// Restrict the query to the worker that originally received the query
    Local,
    /// Number of times the query can be "broadcasted" to all visible workers
    Broadcasts(u8),
    /// Number of times the query will be passed on to another worker.
    ///
    /// # Note
    ///
    /// If the number is smaller than the number of currently visible workers,
    /// a random selection of workers is used.
    Edges(u8),
    /// Number of workers to receive the query
    Workers(u8),
    /// Maximum roundtrip time allowed when considering broadcasting query to
    /// remote workers.
    ///
    /// # Note
    ///
    /// This is a best-effort scope requirement performed based on available
    /// connection metrics data, which can be limited.
    Time(std::time::Duration),
}

// TODO expand beyond only similarly described products
/// Combines multiple products.
pub fn combine_products(mut products: Vec<QueryProduct>) -> QueryProduct {
    let mut final_product = match products.pop() {
        Some(p) => p,
        None => return QueryProduct::Empty,
    };

    for product in products {
        match &mut final_product {
            QueryProduct::AddressedVar(map) => match product.into() {
                QueryProduct::AddressedVar(_map) => {
                    for (k, v) in _map {
                        if !map.contains_key(&k) {
                            map.insert(k, v);
                        }
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }

    final_product
}

pub async fn process_query(
    query: &Query,
    part: &mut crate::worker::part::Part,
) -> Result<QueryProduct> {
    // HACK
    debug!(">> all entities: {:?}", part.entities);
    let mut selected_entities = part.entities.keys().map(|v| v).collect::<Vec<&StringId>>();
    let entities = &part.entities;
    // println!(
    //     "copying all entity keys took: {} ms",
    //     Instant::now().duration_since(insta).as_millis()
    // );

    // first apply filters and get a list of selected entities
    for filter in &query.filters {
        let mut to_retain = Vec::new();
        let insta = Instant::now();
        match filter {
            Filter::Id(desired_ids) => {
                unimplemented!();
                // for selected_entity_id in &selected_entities {
                //     if !desired_ids.contains(&selected_entity_id) {
                //         continue;
                //     }
                //     to_retain.push(*selected_entity_id);
                // }
            }
            Filter::Name(desired_names) => {
                for selected_entity in &selected_entities {
                    if !desired_names.contains(selected_entity) {
                        continue;
                    }
                    to_retain.push(selected_entity.to_owned());
                }
            }
            Filter::AllComponents(desired_components) => {
                unimplemented!();
                // 'ent: for entity_id in &selected_entities {
                //     // 'ent: for (entity_id, entity) in entities {
                //     if let Some(entity) = entities.get(entity_id) {
                //         for desired_component in desired_components {
                //             if !entity.components.contains(desired_component) {
                //                 continue 'ent;
                //             }
                //         }
                //         to_retain.push(*entity_id);
                //     }
                // }
            }
            Filter::Distance(x_addr, y_addr, z_addr, dx, dy, dz) => {
                unimplemented!();
                // // first get the target point position
                // let entity_id = match entity_names.get(&x_addr.entity) {
                //     Some(entity_id) => *entity_id,
                //     None => match x_addr.entity.parse() {
                //         Ok(p) => p,
                //         Err(e) => continue,
                //     },
                // };

                // // let insta = std::time::Instant::now();
                // let (x, y, z) = if let Some(entity) = entities.get(&entity_id) {
                //     (
                //         entity
                //             .storage
                //             .get_var(&x_addr.storage_index())
                //             .unwrap()
                //             .clone()
                //             .to_float(),
                //         entity
                //             .storage
                //             .get_var(&y_addr.storage_index())
                //             .unwrap()
                //             .clone()
                //             .to_float(),
                //         entity
                //             .storage
                //             .get_var(&z_addr.storage_index())
                //             .unwrap()
                //             .clone()
                //             .to_float(),
                //     )
                // } else {
                //     unimplemented!();
                // };
                // // println!(
                // //     "getting xyz took: {} ms",
                // //     Instant::now().duration_since(insta).as_millis()
                // // );

                // // let insta = std::time::Instant::now();
                // for entity_id in &selected_entities {
                //     if let Some(entity) = entities.get(entity_id) {
                //         if let Ok(pos_x) = entity
                //             .storage
                //             .get_var(&("transform".parse().unwrap(), "pos_x".parse().unwrap()))
                //         {
                //             if (pos_x.to_float() - x).abs() > *dx {
                //                 continue;
                //             }
                //         }
                //         if let Ok(pos_y) = entity
                //             .storage
                //             .get_var(&("transform".parse().unwrap(), "pos_y".parse().unwrap()))
                //         {
                //             if (pos_y.to_float() - y).abs() > *dy {
                //                 continue;
                //             }
                //         }
                //         if let Ok(pos_z) = entity
                //             .storage
                //             .get_var(&("transform".parse().unwrap(), "pos_z".parse().unwrap()))
                //         {
                //             if (pos_z.to_float() - z).abs() > *dz {
                //                 continue;
                //             }
                //         }
                //         to_retain.push(*entity_id);
                //     }
                // }
                // // println!(
                // //     "iterating entities took: {} ms",
                // //     Instant::now().duration_since(insta).as_millis()
                // // );
            }
            Filter::DistanceMultiPoint(multi) => {
                unimplemented!();
                // for (x_addr, y_addr, z_addr, dx, dy, dz) in multi {
                //     // first get the target point position
                //     let entity_id = match entity_names.get(&x_addr.entity) {
                //         Some(entity_id) => *entity_id,
                //         None => x_addr.entity.parse().unwrap(),
                //     };
                //     let (x, y, z) = if let Some(entity) = entities.get(&entity_id) {
                //         (
                //             entity
                //                 .storage
                //                 .get_var(&x_addr.storage_index())
                //                 .unwrap()
                //                 .clone()
                //                 .to_float(),
                //             entity
                //                 .storage
                //                 .get_var(&y_addr.storage_index())
                //                 .unwrap()
                //                 .clone()
                //                 .to_float(),
                //             entity
                //                 .storage
                //                 .get_var(&z_addr.storage_index())
                //                 .unwrap()
                //                 .clone()
                //                 .to_float(),
                //         )
                //     } else {
                //         unimplemented!();
                //     };

                //     for entity_id in &selected_entities {
                //         if let Some(entity) = entities.get(entity_id) {
                //             if let Ok(pos_x) = entity
                //                 .storage
                //                 .get_var(&("transform".parse().unwrap(), "pos_x".parse().unwrap()))
                //             {
                //                 if (pos_x.to_float() - x).abs() > *dx {
                //                     continue;
                //                 }
                //             }
                //             if let Ok(pos_y) = entity
                //                 .storage
                //                 .get_var(&("transform".parse().unwrap(), "pos_y".parse().unwrap()))
                //             {
                //                 if (pos_y.to_float() - y).abs() > *dy {
                //                     continue;
                //                 }
                //             }
                //             if let Ok(pos_z) = entity
                //                 .storage
                //                 .get_var(&("transform".parse().unwrap(), "pos_z".parse().unwrap()))
                //             {
                //                 if (pos_z.to_float() - z).abs() > *dz {
                //                     continue;
                //                 }
                //             }
                //             to_retain.push(*entity_id);
                //         }
                //     }
                // }
            }
            Filter::Node(node_id) => to_retain = selected_entities,
            Filter::Component(comp_name) => {
                for (id, entity) in &part.entities {
                    if entity.components.contains(comp_name) {
                        to_retain.push(id);
                    }
                }
            }
            Filter::SomeComponents(_) => todo!(),
            Filter::VarRange(addr, low, high) => {
                for entity_name in selected_entities {
                    let entity = if let Some(entity) = part.entities.get(entity_name) {
                        entity
                    } else {
                        continue;
                    };
                    let var = if let Ok(var) = entity.storage.get_var(&addr.storage_index()) {
                        var
                    } else {
                        continue;
                    };

                    match addr.var_type {
                        VarType::String => todo!(),
                        VarType::Int => {
                            let int = var.as_int()?;
                            if int > &low.to_int() && int < &high.to_int() {
                                to_retain.push(entity_name);
                            }
                        }
                        VarType::Float => todo!(),
                        VarType::Bool => todo!(),
                        _ => unimplemented!(),
                    }
                }
            }
            Filter::AttrRange(_, _, _) => todo!(),
        }

        selected_entities = to_retain;
    }

    trace!("query: selected entities: {:?}", selected_entities);

    // let insta = std::time::Instant::now();
    let mut mapped_data = FnvHashMap::default();
    for entity_id in selected_entities {
        for mapping in &query.mappings {
            match mapping {
                Map::All => {
                    if let Some(entity) = entities.get(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            mapped_data.insert((entity_id, comp_name, var_name), var);
                        }
                    }
                    // we've selected everything, disregard other mappings
                    break;
                }
                Map::Var(map_var_type, map_var_name) => {
                    if let Some(entity) = entities.get(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            if &var.get_type() == map_var_type && var_name == map_var_name {
                                mapped_data.insert((entity_id, comp_name, var_name), var);
                            }
                        }
                    }
                }
                Map::VarName(map_var_name) => {
                    if let Some(entity) = entities.get(entity_id) {
                        for ((comp_name, var_name), var) in &entity.storage.map {
                            if var_name == map_var_name {
                                mapped_data.insert((entity_id, comp_name, var_name), var);
                            }
                        }
                    }
                }
                Map::Components(map_components) => {
                    for map_component in map_components {
                        if let Some(entity) = entities.get(entity_id) {
                            for ((comp_name, var_name), var) in &entity.storage.map {
                                if comp_name == map_component {
                                    mapped_data.insert((entity_id, comp_name, var_name), var);
                                }
                            }
                        }
                    }
                }
                _ => unimplemented!(),
            }
        }
    }

    trace!("mapped_data: {:?}", mapped_data);
    // println!(
    //     "mapping took: {} ms",
    //     Instant::now().duration_since(insta).as_millis()
    // );

    // let insta = std::time::Instant::now();
    let mut query_product = QueryProduct::Empty;
    match query.description {
        Description::None => match query.layout {
            Layout::Var => {
                query_product = QueryProduct::Var(
                    mapped_data
                        .into_iter()
                        .map(|(_, var)| var.clone())
                        .collect(),
                );
            }
            _ => unimplemented!(),
        },
        Description::NativeDescribed => match query.layout {
            Layout::Var => {
                unimplemented!();
                // query_product = QueryProduct::NativeAddressedVar(
                //     mapped_data
                //         .into_iter()
                //         .map(|((ent_id, comp_name, var_name), var)| {
                //             ((*ent_id, comp_name.clone(), var_name.clone()), var.clone())
                //         })
                //         .collect(),
                // );
            }
            _ => unimplemented!(),
        },
        Description::Addressed => match query.layout {
            Layout::Var => {
                let mut data = FnvHashMap::default();
                for ((ent_id, comp_name, var_name), var) in mapped_data {
                    let addr = Address {
                        // TODO make it optional to search for entity string name
                        // entity: entity_names
                        //     .iter()
                        //     .find(|(name, id)| id == &ent_id)
                        //     .map(|(name, _)| *name)
                        //     .unwrap_or(ent_id.to_string().parse().unwrap()),
                        entity: ent_id.to_string().parse().unwrap(),
                        component: comp_name.clone(),
                        var_type: var.get_type(),
                        var_name: var_name.clone(),
                    };
                    data.insert(addr, var.clone());
                }
                query_product = QueryProduct::AddressedVar(data);
            }
            Layout::Typed => {
                let mut data = AddressedTypedMap::default();
                for ((ent_id, comp_name, var_name), var) in mapped_data {
                    let addr = Address {
                        // TODO make it optional to search for entity string name
                        // entity: entity_names
                        // .iter()
                        // .find(|(name, id)| id == &ent_id)
                        // .map(|(name, _)| *name)
                        // .unwrap_or(ent_id.to_string().parse().unwrap()),
                        entity: ent_id.to_string().parse().unwrap(),
                        component: comp_name.clone(),
                        var_type: var.get_type(),
                        var_name: var_name.clone(),
                    };
                    if var.is_float() {
                        data.floats.insert(addr, var.to_float());
                    } else if var.is_bool() {
                        data.bools.insert(addr, var.to_bool());
                    } else if var.is_int() {
                        data.ints.insert(addr, var.to_int());
                    }
                }
                query_product = QueryProduct::AddressedTyped(data);
            }
            _ => unimplemented!(),
        },
        _ => unimplemented!(),
    }

    trace!("query_product: {:?}", query_product);

    // println!(
    //     "packing took: {} ms",
    //     Instant::now().duration_since(insta).as_millis()
    // );

    Ok(query_product)
}
