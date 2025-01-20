//! Data query system.

mod process;

pub use process::process_query;

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use fnv::FnvHashMap;

use crate::address::LocalAddress;
use crate::entity::Entity;
use crate::time::Instant;
use crate::{
    string, Address, CompName, EntityId, EntityName, EventName, Float, Int, Result, StringId, Var,
    VarName, VarType,
};

/// Collection of items defining a complex simulation data query.
// TODO: consider moving subscription triggers out of the query structure
// itself. The system should most store the query structure per registered
// subscription so storing the subscription trigger here is not helpful.
// This however will require to set up a separate interface for registering
// subscription queries.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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

impl Default for Query {
    fn default() -> Self {
        Self {
            trigger: Trigger::default(),
            description: Description::default(),
            layout: Layout::default(),
            /// By default all entities are selected, there are no filters.
            filters: vec![],
            /// By default all the entity variables are included in the
            /// response.
            // TODO: This is useful for testing but should probably not be the
            // case on release, as it can result in huge responses for larger
            // simulations.
            mappings: vec![Map::All],
            scope: Scope::default(),
        }
    }
}

/// Uniform query product type.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum QueryProduct {
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

    pub fn to_map(self) -> Result<FnvHashMap<Address, Var>> {
        match self {
            QueryProduct::AddressedVar(map) => Ok(map),
            QueryProduct::Var(vec) => Err(crate::Error::FailedConversion(
                "can't make query product into a map, lacking address information".to_string(),
            )),
            _ => unimplemented!(),
        }
    }
}

/// Defines possible trigger conditions for a query.
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

/// Defines all possible entity filtering mechanisms when processing a query.
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

/// Defines all possible ways to map entity data, allowing for a fine-grained
/// control over the resulting query product.
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

/// Defines different ways of structuring the query product in terms of
/// associating data points with their addresses.
// TODO: research how much of fine-grained control is necessary here.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub enum Description {
    /// Describe values with an address tuple
    NativeDescribed,
    /// Describe values with an address structure
    Addressed,
    /// Describe values with with an entity name
    Entity,
    /// Describe values with with a component name
    Component,
    /// Describe values with a variable name
    Var,
    /// Describe values with a (component, var) tuple
    ComponentVar,
    /// Values ordered based on an order table
    Ordered,
    #[default]
    None,
}

/// Defines possible layout versions for the returned data.
#[derive(Copy, Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub enum Layout {
    /// Use the internal value representation type built on Rust's enum
    #[default]
    Var,
    /// Coerce all values to strings
    String,
    /// Use a separate map/list for each variable type
    Typed,
}

/// Defines how widely should the search be spread across the cluster.
///
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

/// Combines multiple products.
// TODO: expand beyond only similarly described products
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
