use std::io::Read;

use fnv::FnvHashMap;

use crate::model::{
    Component, DataEntry, DataFileEntry, DataImageEntry, EntitySpawnRequestModel, EventModel,
    PrefabModel, ServiceModel,
};
use crate::{util, Model, Result, MODULE_MANIFEST_FILE};

/// Fully processed representation of a module.
///
/// # Self-contained model
///
/// Each module is processed into a `SimModel`. Later during scenario-loading
/// multiple modules can be selected, leading to merging of the `SimModel`s
/// in a particular order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Module {
    pub manifest: ModuleManifest,
    pub model: Model,
}

impl Module {
    /// Creates module structure from provided directory.
    ///
    /// # Virtual file system
    ///
    /// Module file structure is provided as a vfs filesystem. This way it can
    /// be easily created not only from regular physical filesystem, but also
    /// from in-memory filesystems, making it easier in case of running from
    /// archive for example.
    pub fn from_files<FS: vfs::FileSystem>(fs: &FS, module_root: Option<&str>) -> Result<Self> {
        let manifest = ModuleManifest::from_dir_at(fs, module_root.unwrap_or(""))?;
        let model = Model::from_files(fs, module_root)?;

        debug!("manifest: {:?}", manifest);
        debug!("model: {:?}", model);

        Ok(Self { manifest, model })
        // unimplemented!()
    }
}

/// Module manifest model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ModuleManifest {
    // required
    /// Module name
    pub name: String,
    /// Module version
    pub version: String,
    /// Required engine version
    pub engine_version_req: String,
    /// Required engine features
    pub engine_features: Vec<String>,
    /// List of other module dependencies for this module
    pub dependencies: FnvHashMap<String, ModuleDep>,
    /// List of required target addrs
    pub reqs: Vec<String>,

    pub libraries: Vec<ModuleLib>,
    pub services: Vec<ServiceModel>,

    // optional
    /// Free-form module name
    pub title: Option<String>,
    /// Module description
    pub desc: Option<String>,
    /// Longer module description
    pub desc_long: Option<String>,
    /// Author information
    pub author: Option<String>,
    /// Website information
    pub website: Option<String>,
}

impl ModuleManifest {
    /// Create module manifest from path to module directory
    pub fn from_dir_at<FS: vfs::FileSystem>(fs: &FS, dir_path: &str) -> Result<Self> {
        let manifest_path = format!("{}/{}", dir_path, MODULE_MANIFEST_FILE);
        let deser_manifest: deser::ModuleManifest =
            util::deser_struct_from_path(fs, &manifest_path)?;
        let mut dep_map: FnvHashMap<String, ModuleDep> = FnvHashMap::default();
        for (name, value) in deser_manifest.dependencies {
            // TODO
            // dep_map.insert(name.clone(),
            // ModuleDep::from_toml_value(&name,
            // &value));
        }
        let mut req_vec: Vec<String> = Vec::new();
        for req in deser_manifest.reqs {
            req_vec.push(req);
        }
        let mut engine_version_req = String::new();
        let mut engine_features = Vec::new();
        if let Some(table) = deser_manifest._mod.engine.as_table() {
            for (name, value) in table {
                match name.as_str() {
                    "version" => engine_version_req = value.as_str().unwrap().to_string(),
                    "features" => {
                        engine_features = value
                            .as_array()
                            .expect("`features` entry must be an array")
                            .iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect()
                    }
                    _ => (),
                }
            }
        } else if let Some(s) = deser_manifest._mod.engine.as_str() {
            engine_version_req = s.to_string();
        }
        let mut libs = Vec::new();
        for (lib_name, lib_value) in deser_manifest.libraries {
            let mut library_path = None;
            let mut project_path = None;
            let mut project_mode = None;
            let mut project_features = None;
            let mut project_inherit_features = false;
            let mut project_env = None;
            if let Some(table) = lib_value.as_table() {
                for (name, value) in table {
                    match name.as_str() {
                        "path" | "library" => {
                            library_path = Some(value.as_str().unwrap().to_string())
                        }
                        "project" => {
                            if let Some(project_table) = value.as_table() {
                                for (name, value) in project_table {
                                    match name.as_str() {
                                        "path" => {
                                            project_path = Some(value.as_str().unwrap().to_string())
                                        }
                                        "mode" => {
                                            project_mode = Some(value.as_str().unwrap().to_string())
                                        }
                                        "features" => {
                                            project_features =
                                                Some(value.as_str().unwrap().to_string())
                                        }
                                        "inherit-features" => {
                                            project_inherit_features = value.as_bool().unwrap()
                                        }
                                        "env" => {
                                            project_env = Some(value.as_str().unwrap().to_string())
                                        }
                                        _ => warn!(
                                            "unrecognized option in library definition: {}, module: {}",
                                            name, deser_manifest._mod.name
                                        ),
                                    }
                                }
                            } else {
                                project_path = Some(value.as_str().unwrap().to_string());
                            }
                        }
                        _ => (),
                    }
                }
            } else if let Some(s) = lib_value.as_str() {
                library_path = Some(s.to_string());
            }
            let lib = ModuleLib {
                name: lib_name,
                path: library_path,
                project_path,
                project_mode,
                project_features,
                project_inherit_features,
                project_env,
            };
            libs.push(lib);
        }

        let mut services = Vec::new();
        for (service_name, service_value) in deser_manifest.services {
            let mut executable_path = None;
            let mut project_path = None;
            let mut type_ = None;
            let mut type_args = None;
            let mut managed = true;
            let mut args = Vec::new();
            let mut output = None;

            if let Some(table) = service_value.as_table() {
                for (name, value) in table {
                    match name.as_str() {
                        "type" | "type_" | "policy" => {
                            if let Some(v) = value.as_str() {
                                type_ = Some(v.to_string());
                            } else if let Some(table_) = value.as_table() {
                                for (name_, value_) in table_ {
                                    match name_.as_str() {
                                        "name" => type_ = Some(value_.to_string()),
                                        "args" => type_args = Some(value_.to_string()),
                                        _ => (),
                                    }
                                }
                            }
                        }
                        "executable" | "path" => {
                            let ep = value.to_string()[1..value.to_string().len() - 1].to_string();
                            executable_path = Some(format!("{}/{}", dir_path, ep));
                        }
                        "project" => project_path = Some(value.to_string()),
                        "args" => {
                            if let Some(arr) = value.as_array() {
                                args = arr
                                    .iter()
                                    .map(|v| v.as_str().unwrap().to_string())
                                    .collect();
                            }
                        }
                        "output" => output = Some(value.to_string()),
                        _ => (),
                    }
                }
            } else if let Some(s) = service_value.as_str() {
                let ep = s[1..s.len() - 1].to_string();
                executable_path = Some(format!("{}/{}", dir_path, ep));
            }

            let service = ServiceModel {
                name: service_name,
                type_: None,
                type_args: None,
                executable: executable_path,
                project: project_path,
                managed,
                args,
                output,
            };
            services.push(service);
        }

        Ok(ModuleManifest {
            name: deser_manifest._mod.name,
            engine_version_req,
            engine_features,
            version: deser_manifest._mod.version,
            dependencies: dep_map,
            reqs: req_vec,
            libraries: libs,
            services,
            title: match deser_manifest._mod.title.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            desc: match deser_manifest._mod.desc.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            desc_long: match deser_manifest._mod.desc_long.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            author: match deser_manifest._mod.author.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            website: match deser_manifest._mod.website.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
        })
    }
}

/// Module dependency on another module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ModuleDep {
    pub name: String,
    pub version_req: String,
    pub git_address: Option<String>,
}

/// Library declared by a module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ModuleLib {
    pub name: String,
    /// Path to dynamic library file relative to module root
    pub path: Option<String>,
    /// Path to buildable project
    pub project_path: Option<String>,
    /// Build the project in debug or release mode
    pub project_mode: Option<String>,
    pub project_features: Option<String>,
    pub project_inherit_features: bool,
    pub project_env: Option<String>,
}
