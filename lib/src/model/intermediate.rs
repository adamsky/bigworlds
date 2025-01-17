use std::path::PathBuf;

use fnv::FnvHashMap;

use crate::{util, MODEL_MANIFEST_FILE};
use crate::{Result, StringId};

/// Intermediate model representation. It's existence is predicated by the need
/// to resolve differences between the readily deserializable file structure
/// and the regular model representation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Model {
    #[serde(rename = "model")]
    pub meta: Meta,

    pub dependencies: FnvHashMap<String, Dependency>,

    // #[serde(rename = "scenario")]
    pub scenarios: Vec<Scenario>,

    // #[serde(rename = "behavior")]
    pub behaviors: Vec<Behavior>,

    pub components: Vec<Component>,
    // pub events: Vec<EventModel>,
    // pub scripts: Vec<String>,
    // pub prefabs: Vec<PrefabModel>,
    // pub components: Vec<ComponentModel>,
    pub data: FnvHashMap<String, String>,
    // pub data_files: Vec<DataFileEntry>,
    // pub data_imgs: Vec<DataImageEntry>,
    // pub services: Vec<ServiceModel>,
    // /// Entities to be spawned at startup
    // pub entity_spawn_requests: Vec<EntitySpawnRequestModel>,
}

impl Model {
    /// Creates a new intermediate model using provided file system.
    ///
    /// # Including linked files
    ///
    /// This function will walk the `include` paths and add the data from
    /// files specified there. End result is the full view of the model as read
    /// from the files.
    pub fn from_files<FS: vfs::FileSystem>(fs: &FS, root: Option<&str>) -> Result<Self> {
        // read the manifest at `model.toml` into intermediate representation
        let mut model: Model = util::deser_struct_from_path(fs, MODEL_MANIFEST_FILE)?;

        debug!("Intermediate model manifest: {model:?}");

        // Starting at the manifest, follow the `include`s recursively and
        // merge data from all the files found this way.

        let mut to_include = model.meta.includes.clone();

        // Start the include loop. Once it ends we should have all model files
        // merged into the top level model.
        loop {
            // pop another include
            if let Some(include) = to_include.pop() {
                println!("including from path: {include}");

                // TODO: expand wildcard
                // if include.contains("*") {
                //     let include_path = PathBuf::from(&include.repla);
                //     if include_path.is_dir() {

                //     }
                // }

                // read the include
                let mut include_model: Model = util::deser_struct_from_path(fs, &include)?;
                // println!("include_model at {include}: {include_model:?}");

                // find the parent path for the include path in question
                let include_path = PathBuf::from(include);
                let include_parent = include_path.parent().unwrap();

                // normalize sub-includes' paths to start from parent path
                let normalized_includes = include_model
                    .meta
                    .includes
                    .iter()
                    .map(|i| {
                        //
                        let path = PathBuf::from(i);
                        let path = include_parent.join(path);
                        path.to_str().unwrap().to_string()
                    })
                    .collect::<Vec<_>>();

                // for good measure insert back the normalized includes
                include_model.meta.includes = normalized_includes.clone();

                // add all found includes to the queue
                to_include.extend(normalized_includes);

                // merge the include into the main model
                model.merge(include_model);
            } else {
                break;
            }
        }

        Ok(model)
    }

    pub fn merge(&mut self, other: Model) {
        self.meta.merge(other.meta);
        self.scenarios.extend(other.scenarios);
        self.behaviors.extend(other.behaviors);
        self.components.extend(other.components);
        self.data.extend(other.data);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub name: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    #[serde(rename = "include")]
    pub includes: Vec<String>,
}

impl Meta {
    pub fn merge(&mut self, other: Meta) {
        if self.name.is_none() {
            self.name = other.name;
        }
        if self.description.is_none() {
            self.description = other.description;
        }
        if self.author.is_none() {
            self.author = other.author;
        }
        self.includes.extend(other.includes);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Dependency {
    pub path: Option<String>,
    pub git: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Scenario {
    pub name: String,
    pub data: FnvHashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Behavior {
    pub name: String,
    pub r#type: String,
    pub path: String,
    pub entry: String,
    pub lib: String,
    // targets:
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Component {
    pub name: StringId,
    pub vars: FnvHashMap<StringId, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScenarioData {
    Number(i32),
    String(String),
    Map(FnvHashMap<String, String>),
    Csv(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEntry {
    // Simple((String, String)),
    // List((String, Vec<String>)),
    // Grid((String, Vec<Vec<String>>)),
}
