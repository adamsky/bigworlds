//! Contains structs used for procedural deserialization.

use std::collections::HashMap;

use linked_hash_map::LinkedHashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub project: ProjectManifestMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifestMeta {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleLib {
    path: String,
    build: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    #[serde(rename = "mod")]
    pub _mod: ModuleManifestMod,
    #[serde(default)]
    pub dependencies: HashMap<String, toml::Value>,
    #[serde(default)]
    pub reqs: Vec<String>,
    #[serde(default)]
    pub libraries: HashMap<String, toml::Value>,
    #[serde(default)]
    pub services: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifestMod {
    // required
    pub name: String,
    pub version: String,

    #[serde(default = "default_engine_req")]
    pub engine: toml::Value,

    // optional
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub desc_long: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub website: String,
}

fn default_engine_req() -> toml::Value {
    toml::Value::String("*".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScenarioManifest {
    pub scenario: ScenarioManifestScenario,
    #[serde(default)]
    pub mods: LinkedHashMap<String, toml::Value>,
    #[serde(default)]
    pub settings: HashMap<String, toml::Value>,
    #[serde(default)]
    pub services: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioManifestScenario {
    // required
    pub name: String,
    pub version: String,

    #[serde(default = "default_engine_version")]
    pub engine: String,

    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub desc_long: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub website: String,
}

fn default_engine_version() -> String {
    "*".to_string()
}

// TODO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofManifest {
    // required
    name: String,
    version: String,
    bigworlds: String,
    scenario: String,
    // optional
    #[serde(default)]
    title: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    desc_long: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    website: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFile {
    #[serde(default)]
    #[serde(rename = "comp")]
    pub components: Vec<ComponentEntry>,
    #[serde(default)]
    #[serde(rename = "prefab")]
    pub prefabs: Vec<PrefabEntry>,
    #[serde(default)]
    #[serde(rename = "ent")]
    pub entities: Vec<EntityEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vars: Vec<String>,
    #[serde(default)]
    pub states: Vec<ComponentStateEntry>,
    #[serde(default)]
    pub start_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStateEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cmds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub comps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityEntry {
    #[serde(default)]
    pub prefab: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub default: Option<String>,
}
