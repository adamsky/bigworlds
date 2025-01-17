//! Model content definitions, logic for turning deserialized data into
//! model objects.

#![allow(unused)]

// mod deser;
mod intermediate;

// mod module;
// mod scenario;

use std::collections::HashMap;
use std::fs::{read, read_dir, File};
use std::hash::Hash;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fnv::FnvHashMap;
use semver::{Version, VersionReq};
use toml::Value;

use crate::address::{Address, LocalAddress, ShortLocalAddress};
use crate::{
    string, var, CompName, EntityName, EventName, PrefabName, ShortString, StringId, VarName,
    VarType,
};

use crate::error::{Error, Result};
use crate::{util, MODEL_MANIFEST_FILE};

#[cfg(feature = "machine_script")]
use crate::machine::script::{parser, preprocessor, InstructionType};
#[cfg(feature = "machine")]
use crate::machine::START_STATE_NAME;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Behavior {
    pub lib_path: String,
    /// Name of the entry function
    pub entry: String,
}

impl From<intermediate::Behavior> for Behavior {
    fn from(i: intermediate::Behavior) -> Self {
        Self {
            lib_path: i.lib,
            entry: i.entry,
        }
    }
}

/// Collection of all the information defining an actionable simulation.
///
/// # Runtime mutability
///
/// The engine allows for large amount of runtime flexibility, including
/// introduction of changes to the globally distributed model as the simulation
/// is being run.
///
/// There are a few caveats.
///
/// Models that heavily depend on highly specialized behaviors and/or services
/// where certain model characteristics are expected, may enjoy less
/// flexibility in that regard. On the other hand those behaviors and services
/// are also allowed to introduce changes to the global model, so it's possible
/// to account for this in some respects.
///
/// Changes to the global model mean the need to perform global synchronization
/// of the model accross all the nodes. This can prove to be a relatively slow
/// operation, especially cumbersome in situations where it's necessary to
/// maintain high frequency updates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Model {
    pub scenarios: Vec<Scenario>,
    pub events: Vec<EventModel>,
    // pub scripts: Vec<String>,
    pub prefabs: Vec<PrefabModel>,
    pub components: Vec<Component>,
    pub behaviors: Vec<Behavior>,
    pub data: Vec<DataEntry>,
    pub data_files: Vec<DataFileEntry>,
    // pub data_imgs: Vec<DataImageEntry>,
    // pub services: Vec<ServiceModel>,

    // /// Entities to be spawned at startup
    // pub entity_spawn_requests: Vec<EntitySpawnRequestModel>,
}

/// Defines translation from intermediate to regular model definition.
/// Includes a bunch of spaghetti code.
impl TryFrom<intermediate::Model> for Model {
    type Error = Error;

    fn try_from(im: intermediate::Model) -> Result<Self> {
        println!("intermediate model: {:?}", im);

        let mut model = Self::default();

        for im_scenario in im.scenarios {
            model.scenarios.push(Scenario {
                name: im_scenario.name,
            })
        }

        for im_component in im.components {
            let mut component = Component::default();
            component.name = im_component.name;
            for (name, var) in im_component.vars {
                component.vars.push(Var {
                    name,
                    type_: VarType::String,
                    default: None,
                });
            }
            model.components.push(component);
        }

        for im_behavior in im.behaviors {
            // TODO: At this point we need to produce artifacts, so build rust
            // projects, etc.

            model.behaviors.push(im_behavior.into());
        }

        Ok(model)
    }
}

impl Model {
    /// Merges another `Model` into `self`.
    pub fn merge(&mut self, other: Self) -> Result<()> {
        // self.events.extend(other.events);
        // self.scripts.extend(other.scripts);
        // self.prefabs.extend(other.prefabs);
        // self.components.extend(other.components);
        // self.data.extend(other.data);
        // self.data_files.extend(other.data_files);
        // self.data_imgs.extend(other.data_imgs);
        // self.services.extend(other.services);
        // self.entity_spawn_requests
        //     .extend(other.entity_spawn_requests);
        Ok(())
    }

    /// Creates a new simulation model using provided file system.
    pub fn from_files<FS: vfs::FileSystem>(fs: &FS, root: Option<&str>) -> Result<Self> {
        // Create an intermediate model from files. We end up with the full
        // view of the model in the raw form as read from the model files.
        // In the subsequent steps the intermediate form gets normalized into
        // the regular model representation.
        let intermediate = intermediate::Model::from_files(fs, root)?;
        debug!("Intermediate model: {intermediate:?}");

        // create an empty sim model to work on
        let mut model = Model::try_from(intermediate)?;

        // load from scripts
        #[cfg(feature = "machine_script")]
        {
            // model.events.push(EventModel {
            //     id: string::new_truncate("_scr_init"),
            // });

            // let scr_init_mod_template = ComponentModel {
            //     name: string::new_truncate("_init_mod_"),
            //     ..ComponentModel::default()
            // };

            // let scr_init_mod_template = ComponentModel {
            //     name: StringId::from_unchecked("_init_mod_"),
            //     vars: vec![],
            //     start_state: StringId::from_unchecked("main"),
            //     triggers: vec![StringId::from_unchecked("_scr_init")],
            //     logic: LogicModel {
            //         commands: Vec::new(),
            //         states: FnvHashMap::default(),
            //         procedures: FnvHashMap::default(),
            //         cmd_location_map: FnvHashMap::default(),
            //         pre_commands: FnvHashMap::default(),
            //     },
            //     source_files: Vec::new(),
            //     script_files: Vec::new(),
            //     lib_files: Vec::new(),
            // };
            // #[cfg(feature = "machine")]
            // use crate::machine::cmd::Command;
            // use crate::machine::{CommandPrototype, LocationInfo};

            // // use script processor to handle scripts
            // let program_data = crate::machine::script::util::get_program_metadata();

            // // create path to entry script
            // let mod_entry_file_path = PathBuf::new().join(module_root).join(format!(
            //     "{}{}",
            //     crate::MODULE_ENTRY_FILE_NAME,
            //     crate::machine::script::SCRIPT_FILE_EXTENSION
            // ));

            // // parse the module entry script
            // let mut instructions = parser::parse_script_at(
            //     fs,
            //     &mod_entry_file_path.to_string_lossy(),
            //     // TODO
            //     // &scenario.path,
            //     "",
            // )?;

            // // preprocess entry script
            // preprocessor::run(&mut instructions, &mut model, &program_data)?;

            // // turn instructions into proper commands
            // let mut commands: Vec<Command> = Vec::new();
            // let mut cmd_prototypes: Vec<CommandPrototype> = Vec::new();
            // let mut cmd_locations: Vec<LocationInfo> = Vec::new();
            // // first get a list of commands from the main instruction list
            // for instruction in instructions {
            //     let cmd_prototype = match instruction.type_ {
            //         InstructionType::Command(c) => c,
            //         _ => continue,
            //     };
            //     cmd_prototypes.push(cmd_prototype);
            //     cmd_locations.push(instruction.location.clone());
            // }

            // let mut comp_model = scr_init_mod_template.clone();

            // // don't include name in case short_stackid feature is on
            // // comp_model.name = string::new_truncate(&format!("init_{}",
            // // module.manifest.name));
            // comp_model.name = string::new_truncate(&format!("init_{}", 0));

            // for (n, cmd_prototype) in cmd_prototypes.iter().enumerate() {
            //     cmd_locations[n].comp_name = Some(comp_model.name.clone());
            //     cmd_locations[n].line = Some(n);

            //     // create command struct from prototype
            //     let command =
            //         Command::from_prototype(cmd_prototype, &cmd_locations[n], &cmd_prototypes)?;
            //     commands.push(command.clone());

            //     // insert the commands into the component's logic model
            //     if let Command::Procedure(proc) = &command {
            //         comp_model
            //             .logic
            //             .procedures
            //             .insert(proc.name.clone(), (proc.start_line, proc.end_line));
            //     }
            //     comp_model.logic.commands.push(command);
            //     comp_model
            //         .logic
            //         .cmd_location_map
            //         //.insert(comp_model.logic.commands.len() - 1, location.clone());
            //         .insert(n, cmd_locations[n].clone());
            // }

            // comp_model
            //     .logic
            //     .states
            //     .insert(string::new_truncate("main"), (0, commands.len()));
            // mod_init_prefab.components.push(comp_model.name.clone());
            // model.components.push(comp_model);
        }
        // model.prefabs.push(mod_init_prefab);

        Ok(model)
    }

    // /// Creates a new simulation model from a scenario structure.
    // pub fn from_scenario(scenario: Scenario) -> Result<Self> {
    //     // first create an empty sim model
    //     let mut model = SimModel {
    //         events: Vec::new(),
    //         scripts: Vec::new(),
    //         prefabs: Vec::new(),
    //         components: Vec::new(),
    //         data: Vec::new(),
    //         data_files: Vec::new(),
    //         data_imgs: Vec::new(),
    //         services: Vec::new(),
    //         entity_spawn_requests: Vec::new(),
    //     };
    //
    //     // add hardcoded content
    //     #[cfg(feature = "machine")]
    //     model.events.push(crate::model::EventModel {
    //         id: string::new_truncate(crate::DEFAULT_STEP_EVENT),
    //     });
    //
    //     let mut mod_init_prefab = PrefabModel {
    //         name: string::new_truncate("_mods_init"),
    //         ..PrefabModel::default()
    //     };
    //
    //     // iterate over scenario modules
    //     for (module_n, module) in scenario.modules.iter().enumerate() {
    //         // services
    //         for module_service in &module.manifest.services {
    //             model.services.push(module_service.clone());
    //         }
    //
    //         let module_path = PathBuf::from_str(&module.path).unwrap();
    //
    //         // load structured data from toml files
    //         let files = util::find_files_with_extension(
    //             fs,
    //             module_path,
    //             vec!["toml", "tml"],
    //             true,
    //             Some(vec!["Cargo".to_string()]),
    //         );
    //         debug!("toml files: {:?}", files);
    //         for file in files {
    //             match util::deser_struct_from_path(file.clone()) {
    //                 Ok(file_struct) => {
    //                     trace!("toml file struct: {:?}", file_struct);
    //                     model.apply_from_structured_file(file_struct)?;
    //                 }
    //                 Err(e) => {
    //                     warn!(
    //                         "unable to parse file: {}: {}",
    //                         file.to_string_lossy(),
    //                         e.to_string()
    //                     );
    //                 }
    //             }
    //         }
    //
    //         #[cfg(feature = "yaml")]
    //         {
    //             let module_path = PathBuf::from_str(&module.path).unwrap();
    //             let files =
    //                 util::find_files_with_extension(module_path, vec!["yaml", "yml"], true, None);
    //             debug!("yaml files: {:?}", files);
    //             for file in files {
    //                 match util::deser_struct_from_path(file.clone()) {
    //                     Ok(file_struct) => {
    //                         trace!("yaml file struct: {:?}", file_struct);
    //                         model.apply_from_structured_file(file_struct)?;
    //                     }
    //                     Err(e) => {
    //                         warn!(
    //                             "unable to parse file: {}: {}",
    //                             file.to_string_lossy(),
    //                             e.to_string()
    //                         );
    //                     }
    //                 }
    //             }
    //         }
    //
    //         // load from scripts
    //         #[cfg(feature = "machine_script")]
    //         {
    //             model.events.push(EventModel {
    //                 id: string::new_truncate("_scr_init"),
    //             });
    //
    //             let scr_init_mod_template = ComponentModel {
    //                 name: string::new_truncate("_init_mod_"),
    //                 triggers: vec![string::new_truncate("_scr_init")],
    //                 logic: LogicModel {
    //                     start_state: string::new_truncate("main"),
    //                     ..Default::default()
    //                 },
    //                 ..ComponentModel::default()
    //             };
    //
    //             // let scr_init_mod_template = ComponentModel {
    //             //     name: StringId::from_unchecked("_init_mod_"),
    //             //     vars: vec![],
    //             //     start_state: StringId::from_unchecked("main"),
    //             //     triggers: vec![StringId::from_unchecked("_scr_init")],
    //             //     logic: LogicModel {
    //             //         commands: Vec::new(),
    //             //         states: FnvHashMap::default(),
    //             //         procedures: FnvHashMap::default(),
    //             //         cmd_location_map: FnvHashMap::default(),
    //             //         pre_commands: FnvHashMap::default(),
    //             //     },
    //             //     source_files: Vec::new(),
    //             //     script_files: Vec::new(),
    //             //     lib_files: Vec::new(),
    //             // };
    //             // #[cfg(feature = "machine")]
    //             use crate::machine::cmd::Command;
    //             use crate::machine::{CommandPrototype, LocationInfo};
    //
    //             // use script processor to handle scripts
    //             let program_data = crate::machine::script::util::get_program_metadata();
    //
    //             // create path to entry script
    //             let mod_entry_file_path = PathBuf::new()
    //                 .join(crate::MODULES_DIR_NAME)
    //                 .join(&module.manifest.name)
    //                 .join(format!(
    //                     "{}{}",
    //                     crate::MODULE_ENTRY_FILE_NAME,
    //                     crate::machine::script::SCRIPT_FILE_EXTENSION
    //                 ));
    //
    //             // parse the module entry script
    //             let mut instructions = parser::parse_script_at(
    //                 &mod_entry_file_path.to_string_lossy(),
    //                 // TODO
    //                 // &scenario.path,
    //                 "",
    //             )?;
    //
    //             // preprocess entry script
    //             preprocessor::run(&mut instructions, &mut model, &program_data)?;
    //
    //             // turn instructions into proper commands
    //             let mut commands: Vec<Command> = Vec::new();
    //             let mut cmd_prototypes: Vec<CommandPrototype> = Vec::new();
    //             let mut cmd_locations: Vec<LocationInfo> = Vec::new();
    //             // first get a list of commands from the main instruction list
    //             for instruction in instructions {
    //                 let cmd_prototype = match instruction.type_ {
    //                     InstructionType::Command(c) => c,
    //                     _ => continue,
    //                 };
    //                 cmd_prototypes.push(cmd_prototype);
    //                 cmd_locations.push(instruction.location.clone());
    //             }
    //
    //             let mut comp_model = scr_init_mod_template.clone();
    //
    //             // don't include name in case short_stackid feature is on
    //             // comp_model.name = string::new_truncate(&format!("init_{}",
    //             // module.manifest.name));
    //             comp_model.name = string::new_truncate(&format!("init_{}", module_n));
    //
    //             for (n, cmd_prototype) in cmd_prototypes.iter().enumerate() {
    //                 cmd_locations[n].comp_name = Some(comp_model.name.clone());
    //                 cmd_locations[n].line = Some(n);
    //
    //                 // create command struct from prototype
    //                 let command =
    //                     Command::from_prototype(cmd_prototype, &cmd_locations[n], &cmd_prototypes)?;
    //                 commands.push(command.clone());
    //
    //                 // insert the commands into the component's logic model
    //                 if let Command::Procedure(proc) = &command {
    //                     comp_model
    //                         .logic
    //                         .procedures
    //                         .insert(proc.name.clone(), (proc.start_line, proc.end_line));
    //                 }
    //                 comp_model.logic.commands.push(command);
    //                 comp_model
    //                     .logic
    //                     .cmd_location_map
    //                     //.insert(comp_model.logic.commands.len() - 1, location.clone());
    //                     .insert(n, cmd_locations[n].clone());
    //             }
    //
    //             comp_model
    //                 .logic
    //                 .states
    //                 .insert(string::new_truncate("main"), (0, commands.len()));
    //             mod_init_prefab.components.push(comp_model.name.clone());
    //             model.components.push(comp_model);
    //         }
    //     }
    //     model.prefabs.push(mod_init_prefab);
    //
    //     Ok(model)
    // }
}

impl Model {
    // pub fn apply_from_structured_file(&mut self, file_struct: deser::DataFile) -> Result<()> {
    // for comp_entry in file_struct.components {
    //     trace!("file struct component: {:?}", comp_entry);
    //     let comp_model = ComponentModel::from_deser(comp_entry)?;
    //     self.components.push(comp_model);
    // }
    // for prefab_entry in file_struct.prefabs {
    //     let prefab_model = PrefabModel::from_deser(prefab_entry)?;
    //     self.prefabs.push(prefab_model);
    // }
    // for entity in file_struct.entities {
    //     self.entity_spawn_requests
    //         .push(EntitySpawnRequestModel::from_deser(entity)?);
    // }
    // Ok(())
    // }

    /// Get reference to entity prefab using `type_` and `id` str args.
    pub fn get_prefab(&self, name: &StringId) -> Option<&PrefabModel> {
        self.prefabs
            .iter()
            .find(|entity| &entity.name.as_str() == &name.as_str())
    }

    /// Get mutable reference to entity prefab using `type_` and `id` args.
    pub fn get_entity_mut(&mut self, name: &StringId) -> Option<&mut PrefabModel> {
        self.prefabs.iter_mut().find(|entity| &entity.name == name)
    }

    /// Get reference to component model using `type_` and `id` args.
    pub fn get_component(&self, name: &CompName) -> Result<&Component> {
        self.components
            .iter()
            .find(|comp| &comp.name == name)
            .ok_or(Error::NoComponentModel(name.clone()))
    }

    /// Get mutable reference to component model using `type_` and `id` args.
    pub fn get_component_mut(&mut self, name: &StringId) -> Option<&mut Component> {
        self.components.iter_mut().find(|comp| &comp.name == name)
    }
}

/// Service declared by a module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ServiceModel {
    /// Unique name for the service
    pub name: String,
    pub type_: Option<String>,
    pub type_args: Option<String>,
    /// Path to executable relative to module root
    pub executable: Option<String>,
    /// Path to buildable project
    pub project: Option<String>,
    /// Defines the nature of the service
    pub managed: bool,
    /// Arguments string passed to the executable
    pub args: Vec<String>,
    pub output: Option<String>,
}

// /// Module model.
// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[cfg_attr(
//     feature = "archive",
//     derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
// )]
// pub struct Module {
//     pub manifest: ModuleManifest,
//     pub path: String,
// }
//
// impl Module {
//     pub fn from_dir_at<FS: vfs::FileSystem>(fs: &FS, path: &str) -> Result<Module> {
//         let module_manifest = ModuleManifest::from_dir_at(fs, path)?;
//
//         Ok(Module {
//             manifest: module_manifest,
//             path: path.to_string(),
//         })
//     }
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct EventModel {
    pub id: EventName,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct PrefabModel {
    pub name: PrefabName,
    pub components: Vec<CompName>,
}

// impl PrefabModel {
//     pub fn from_deser(entry: deser::PrefabEntry) -> Result<Self> {
//         Ok(Self {
//             name: string::new_truncate(&entry.name),
//             components: entry
//                 .comps
//                 .iter()
//                 .map(|s| string::new_truncate(s))
//                 .collect(),
//         })
//     }
// }

/// Entity spawn request model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct EntitySpawnRequestModel {
    pub prefab: String,
    pub name: Option<String>,
}

// impl EntitySpawnRequestModel {
//     pub fn from_deser(entry: deser::EntityEntry) -> Result<Self> {
//         Ok(Self {
//             prefab: entry.prefab,
//             name: entry.name,
//         })
//     }
// }

// cfg_if! {
//     if #[cfg(feature = "machine")] {
//         #[derive(Debug, Clone, Serialize, Deserialize)]
//         pub struct ComponentPrefab {
//             pub name: ShortString,
//             pub vars: Vec<VarModel>,
//             pub start_state: ShortString,
//             pub triggers: Vec<ShortString>,
//
//             pub logic: LogicModel,
//
//             pub source_files: Vec<PathBuf>,
//             pub script_files: Vec<PathBuf>,
//             pub lib_files: Vec<PathBuf>,
//         }
//     } else {
//         #[derive(Debug, Clone, Serialize, Deserialize)]
//         pub struct ComponentPrefab {
//             pub name: ShortString,
//             pub vars: Vec<VarModel>,
//         }
//     }
// }
//
/// Component model.
///
/// Components are primarily referenced by their name. Other than that
/// each component defines a list of variables and a list of event triggers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Component {
    /// String identifier of the component
    pub name: CompName,
    /// List of variables that define the component's interface
    pub vars: Vec<Var>,
}

impl From<intermediate::Component> for Component {
    fn from(im: intermediate::Component) -> Self {
        Self {
            name: string::new_truncate(&im.name),
            vars: vec![],
        }
    }
}

// impl Component {
//     pub fn from_deser(comp: deser::ComponentEntry) -> Result<Self> {
//         Ok(Component {
//             name: string::new_truncate(&comp.name),
//             vars: comp
//                 .vars
//                 .into_iter()
//                 // .filter(|(k, v)| v.is_some())
//                 // .map(|(k, v)| VarModel::from_deser(&k, v).unwrap())
//                 // .map(|v| VarModel::from_deser(v).unwrap())
//                 .map(|v| {
//                     VarModel::from_deser(VarEntry {
//                         name: v,
//                         default: None,
//                     })
//                     .unwrap()
//                 })
//                 .collect(),
//         })
//     }
// }

/// Component-bound state machine logic model.
#[cfg(feature = "machine")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct LogicModel {
    /// Name of the starting state
    pub start_state: StringId,
    /// List of local phase commands
    pub commands: Vec<crate::machine::cmd::Command>,
    /// List of pre phase commands
    pub pre_commands: FnvHashMap<ShortString, Vec<crate::machine::cmd::ExtCommand>>,
    /// Mapping of state procedure names to their start and end lines
    pub states: FnvHashMap<StringId, (usize, usize)>,
    /// Mapping of non-state procedure names to their start and end lines
    pub procedures: FnvHashMap<ShortString, (usize, usize)>,
    /// Location info mapped for each command on the list by index
    pub cmd_location_map: Vec<crate::machine::LocationInfo>,
}

#[cfg(feature = "machine")]
impl LogicModel {
    pub fn empty() -> LogicModel {
        LogicModel {
            start_state: string::new_truncate(crate::machine::START_STATE_NAME),
            commands: Vec::new(),
            states: FnvHashMap::default(),
            procedures: FnvHashMap::default(),
            cmd_location_map: Vec::new(),
            pre_commands: FnvHashMap::default(),
        }
    }

    pub fn get_subset(&self, start_line: usize, last_line: usize) -> LogicModel {
        let mut new_logic = LogicModel::empty();
        new_logic.commands = self.commands[start_line..last_line].to_vec();
        new_logic.cmd_location_map = self.cmd_location_map[start_line..last_line].to_vec();
        // warn!("{:?}", new_logic);
        new_logic
    }

    // pub fn from_deser(entry: deser::C) -> Result<VarModel> {}
}

/// Variable model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Var {
    pub name: VarName,
    pub type_: VarType,
    pub default: Option<crate::var::Var>,
}

// impl From<serde_json::Value> for VarModel {}

// impl VarModel {
//     pub fn from_deser(entry: deser::VarEntry) -> Result<VarModel> {
//         let addr = ShortLocalAddress::from_str(&entry.name)?;

//         Ok(VarModel {
//             name: string::new_truncate(&addr.var_name),
//             type_: addr.var_type,
//             default: entry.default.map(|d| Var::from_str(&d, None).unwrap()),
//         })
//     }
// }

/// Data entry model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum DataEntry {
    Simple((String, String)),
    List((String, Vec<String>)),
    Grid((String, Vec<Vec<String>>)),
}

/// Data file entry model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum DataFileEntry {
    Json(String),
    JsonList(String),
    JsonGrid(String),
    Yaml(String),
    YamlList(String),
    YamlGrid(String),
    CsvList(String),
    CsvGrid(String),
}

/// Data image entry model. Used specifically for importing grid data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub enum DataImageEntry {
    BmpU8(String, String),
    BmpU8U8U8(String, String),
    // BmpCombineU8U8U8U8Int(String, String),
    // TODO
    PngU8(String, String),
    PngU8U8U8(String, String),
    PngU8U8U8Concat(String, String),
    // PngCombineU8U8U8U8(String, String),
}
