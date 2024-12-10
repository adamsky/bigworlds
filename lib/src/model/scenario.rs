use fnv::FnvHashMap;
use semver::{Version, VersionReq};
use std::path::PathBuf;
use std::str::FromStr;

// use crate::model::deser;
use crate::model::module::Module;
use crate::{
    util, Error, Result, MODULES_DIR_NAME, MODULE_MANIFEST_FILE, SCENARIOS_DIR_NAME, VERSION,
};

/// Scenario manifest model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ScenarioManifest {
    /// Name of, and unique reference to, the scenario
    pub name: String,
    /// Semver specifier for the scenario version
    pub version: String,
    /// Semver specifier for the engine version
    pub engine: String,

    /// List of the module dependencies for the scenario
    pub mods: Vec<ModuleReq>,
    /// Map of settings, each being essentially an arbitrary data setter
    pub settings: FnvHashMap<String, String>,

    /// More free-form than the name
    pub title: Option<String>,
    /// Short description of the scenario
    pub desc: Option<String>,
    /// Long description of the scenario
    pub desc_long: Option<String>,
    /// Author information
    pub author: Option<String>,
    /// Source website information
    pub website: Option<String>,
}

impl ScenarioManifest {
    /// Creates new scenario manifest object from path reference.
    pub fn from_path<FS: vfs::FileSystem>(
        fs: &FS,
        manifest_path: &str,
    ) -> Result<ScenarioManifest> {
        // let manifest_path = path.join(SCENARIO_MANIFEST_FILE);
        let deser_manifest: deser::ScenarioManifest =
            util::deser_struct_from_path(fs, manifest_path)?;
        let mut mods = Vec::new();
        for module in deser_manifest.mods {
            let (name, value) = module;

            // TODO better errors
            mods.push(ModuleReq::from_toml_value(&name, &value).unwrap());
        }

        Ok(ScenarioManifest {
            name: deser_manifest.scenario.name,
            version: deser_manifest.scenario.version,
            engine: deser_manifest.scenario.engine,

            settings: deser_manifest
                .settings
                .iter()
                .map(|(s, v)| (s.to_string(), v.to_string()))
                .collect(),
            title: match deser_manifest.scenario.title.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            desc: match deser_manifest.scenario.desc.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            desc_long: match deser_manifest.scenario.desc_long.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            author: match deser_manifest.scenario.author.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            website: match deser_manifest.scenario.website.as_str() {
                "" => None,
                s => Some(s.to_owned()),
            },
            mods,
        })
    }
}

/// Scenario module dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct ModuleReq {
    pub name: String,
    pub version_req: String,
    pub git_address: Option<String>,
}

impl ModuleReq {
    /// Create scenario module dependency object from a serde value
    /// representation.
    pub fn from_toml_value(scenario_name: &String, value: &toml::Value) -> Option<ModuleReq> {
        // field names
        let version_field = "version";
        let git_field = "git";

        let mut version_req = "*".to_string();
        let mut git_address = None;

        if let Some(s) = value.as_str() {
            match VersionReq::parse(s) {
                Ok(vr) => version_req = vr.to_string(),
                Err(e) => {
                    warn!(
                        "failed parsing scenario module dep version req \"{}\" ({}), \
                         using default \"*\" (any)",
                        s, e
                    );
                    version_req = VersionReq::STAR.to_string();
                }
            }
        }
        // otherwise it's a mapping with different kinds of entries
        else if let Some(mapping) = value.as_table() {
            unimplemented!();
            if let Ok(vr) = VersionReq::parse(value.as_str().unwrap()) {
                version_req = vr.to_string();
            } else {
                // TODO print warning about the version_req
            }
            // `git_address` is optional, default is `None`
            git_address = match value.get(git_field) {
                Some(v) => Some(String::from(v.as_str().unwrap())),
                None => None,
            };
        } else {
            error!(
                "module dep has to be either a string (version specifier)\
                 or a mapping"
            );
            return None;
        }
        Some(ModuleReq {
            name: scenario_name.clone(),
            version_req,
            git_address,
        })
    }
}

/// Scenario model consisting of the manifest and list of modules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Scenario {
    pub name: String,
    /// Scenario manifest
    pub manifest: ScenarioManifest,
    /// List of module requirements
    pub modules: Vec<ModuleReq>,
}

impl Scenario {
    /// Create a scenario model from a path reference to scenario manifest.
    pub fn from_path<FS: vfs::FileSystem>(fs: &FS, manifest_path: &str) -> Result<Scenario> {
        let _manifest_path = PathBuf::from_str(manifest_path).unwrap();
        println!("manifest_path: {:?}", _manifest_path);
        // resolve project root path
        let mut dir_path = _manifest_path.parent().ok_or(Error::Other(format!(
            "unable to get parent of path: {}",
            manifest_path,
        )))?;
        println!("dir_path: {:?}", dir_path);
        let stem = dir_path.file_stem().ok_or(Error::Other(format!(
            "unable to get stem of path: {}",
            manifest_path
        )))?;
        if stem == SCENARIOS_DIR_NAME {
            dir_path = dir_path.parent().ok_or(Error::Other(format!(
                "unable to get parent of path: {}",
                manifest_path
            )))?;
        } else {
            warn!(
                "scenarios are expected to be kept inside a dedicated \"{}\" directory",
                SCENARIOS_DIR_NAME
            )
        }
        info!("project root directory: {:?}", dir_path);

        // get the scenario manifest
        let scenario_manifest = ScenarioManifest::from_path(fs, manifest_path.clone())?;

        // if the version requirement for the engine specified in
        // the scenario manifest is not met return an error
        if !VersionReq::from_str(&scenario_manifest.engine)?.matches(&Version::from_str(VERSION)?) {
            error!(
                "engine version does not meet the requirement specified in scenario manifest, \
                current engine version: \"{}\", version requirement: \"{}\"",
                VERSION, &scenario_manifest.engine
            );
            return Err(Error::Other(
                "engine version does not match module requirement".to_string(),
            ));
        }

        // get the map of mods to load from the manifest (only mods
        // listed there will be loaded)
        let mods_to_load = scenario_manifest.mods.clone();
        info!(
            "there are {} mods listed in the scenario manifest",
            &mods_to_load.len()
        );

        // // get the path to scenario mods directory
        // let scenario_mods_path = dir_path.join(MODULES_DIR_NAME);
        // // found matching mods will be added to this vec
        // let mut matching_mods: Vec<Module> = Vec::new();
        // // this vec is for storing mod_not_found messages to print
        // // them after the loop
        // let mut mod_not_found_msgs: Vec<String> = Vec::new();
        // // this bool will turn false if any of the mods from the
        // // manifest wasn't found based on it the process of
        // // creating the scenario can be halted
        // let mut all_mods_found = true;
        // // try to find all the mods specified in the scenario
        // // manifest
        // for mod_to_load in mods_to_load {
        //     let mod_to_load_name = mod_to_load.name.clone();
        //     let mod_version_req = mod_to_load.version_req.clone();
        //     let mut found_mod_match = false;
        //     // only the top directories within the mods directory are considered
        //     for mod_dir in util::get_top_dirs_at(fs, scenario_mods_path.to_str().unwrap())? {
        //         let mod_dir_path = PathBuf::from_str(&mod_dir).unwrap();
        //         let mod_dir_name = mod_dir_path.file_name().unwrap().to_str().unwrap();
        //         // we only want matching dir names
        //         if mod_dir_name != mod_to_load_name {
        //             continue;
        //         };
        //         // path of the mod manifest we need to look for
        //         let mod_manifest_path = format!("{}/{}", mod_dir, MODULE_MANIFEST_FILE);
        //         if fs.exists(&mod_manifest_path)? {
        //             let module_manifest: ModuleManifest =
        //                 ModuleManifest::from_dir_at(fs, &mod_dir)?;
        //             // is the engine version requirement met?
        //             if !VersionReq::parse(&module_manifest.engine_version_req)?
        //                 .matches(&Version::parse(VERSION)?)
        //             {
        //                 return Err(Error::Other(format!("mod \"{}\" specifies a version requirement for `bigworlds` (\"engine\" entry) that does not match \
        //                 the version this program is using (version of `bigworlds` used: \"{}\", version requirement: \"{}\")",
        //                                                 module_manifest.name, VERSION, module_manifest.engine_version_req)));
        //             }
        //             // are the engine feature requirements met?
        //             for feature_req in &module_manifest.engine_features {
        //                 match feature_req.as_str() {
        //                     // TODO add more features
        //                     crate::FEATURE_NAME_MACHINE_SYSINFO => {
        //                         if !crate::FEATURE_MACHINE_SYSINFO {
        //                             return Err(Error::RequiredEngineFeatureNotAvailable(
        //                                 crate::FEATURE_NAME_MACHINE_SYSINFO.to_string(),
        //                                 module_manifest.name.clone(),
        //                             ));
        //                         }
        //                     }
        //                     crate::FEATURE_NAME_MACHINE => {
        //                         if !crate::FEATURE_MACHINE {
        //                             return Err(Error::RequiredEngineFeatureNotAvailable(
        //                                 crate::FEATURE_NAME_MACHINE.to_string(),
        //                                 module_manifest.name.clone(),
        //                             ));
        //                         }
        //                     }
        //                     crate::FEATURE_NAME_MACHINE_DYNLIB => {
        //                         if !crate::FEATURE_MACHINE_DYNLIB {
        //                             return Err(Error::RequiredEngineFeatureNotAvailable(
        //                                 crate::FEATURE_NAME_MACHINE_DYNLIB.to_string(),
        //                                 module_manifest.name.clone(),
        //                             ));
        //                         }
        //                     }
        //                     crate::FEATURE_NAME_MACHINE_SCRIPT => {
        //                         if !crate::FEATURE_MACHINE_SCRIPT {
        //                             return Err(Error::RequiredEngineFeatureNotAvailable(
        //                                 crate::FEATURE_NAME_MACHINE_SCRIPT.to_string(),
        //                                 module_manifest.name.clone(),
        //                             ));
        //                         }
        //                     }
        //                     f => unimplemented!(
        //                         "unimplemented engine feature requirement: {}, module: {}",
        //                         f,
        //                         module_manifest.name
        //                     ),
        //                 }
        //             }
        //
        //             // found mod that matches the name and version from scenario manifest
        //             if module_manifest.name == mod_to_load_name
        //                 && VersionReq::parse(&mod_version_req)
        //                     .unwrap_or(VersionReq::STAR)
        //                     .matches(
        //                         &Version::parse(&module_manifest.version)
        //                             .unwrap_or(Version::new(0, 1, 0)),
        //                     )
        //             {
        //                 info!(
        //                     "mod found: \"{}\" version: \"{}\" (\"{}\")",
        //                     mod_to_load_name,
        //                     module_manifest.version.to_string(),
        //                     mod_version_req.to_string()
        //                 );
        //                 let module = match Module::from_dir_at(fs, &mod_dir) {
        //                     Ok(m) => m,
        //                     Err(_) => {
        //                         error!("failed creating module from path: {:?}", mod_dir.clone());
        //                         continue;
        //                     }
        //                 };
        //                 matching_mods.push(module);
        //                 found_mod_match = true;
        //                 break;
        //             }
        //         }
        //     }
        //     // if no matching mod was found
        //     if !found_mod_match {
        //         all_mods_found = false;
        //         mod_not_found_msgs.push(format!(
        //             "mod not found: name:\"{}\" version:\"{}\" specified in scenario manifest was not \
        //                 found",
        //             mod_to_load_name, mod_version_req.to_string()));
        //     }
        // }
        //
        // // check if mod dependencies are present
        // if matching_mods.len() > 0 {
        //     for n in 0..matching_mods.len() - 1 {
        //         let module = &matching_mods[n].clone();
        //         let mut missing_deps: Vec<ModuleDep> = Vec::new();
        //         for (dep_name, dep) in &module.manifest.dependencies {
        //             // is the dependency mod present?
        //             if matching_mods.iter().any(|m| {
        //                 &m.manifest.name == dep_name
        //                     && VersionReq::parse(&dep.version_req)
        //                         .unwrap_or(VersionReq::STAR)
        //                         .matches(
        //                             &Version::parse(&m.manifest.version)
        //                                 .unwrap_or(Version::new(0, 1, 0)),
        //                         )
        //             }) {
        //                 // we're fine
        //             } else {
        //                 // dependency not present, throw an error
        //                 error!(
        //                     "dependency not available: \"{}\" (\"{}\"), \
        //                      required by \"{}\" (\"{}\")",
        //                     dep_name.clone(),
        //                     dep.version_req.to_string(),
        //                     module.manifest.name,
        //                     module.manifest.version.to_string()
        //                 );
        //                 missing_deps.push(dep.clone());
        //                 all_mods_found = false;
        //             }
        //         }
        //         if !missing_deps.is_empty() {
        //             matching_mods.remove(n);
        //         }
        //     }
        // }
        //
        // // show errors about mods not found, they are shown after
        // // the mod found messages
        // for err_msg in mod_not_found_msgs {
        //     error!("{}", err_msg);
        // }
        //
        // // break if not all the mods were found
        // if !all_mods_found {
        //     error!(
        //         "failed to load all mods listed in the scenario manifest ({}/{})",
        //         matching_mods.len(),
        //         mods_to_load.len()
        //     );
        //     // error!("scenario creation process halted: missing modules");
        //     return Err(Error::ScenarioMissingModules);
        // } else {
        //     info!(
        //         "found all mods listed in the scenario manifest ({})",
        //         mods_to_load.len()
        //     );
        // }

        Ok(Scenario {
            name: scenario_manifest.name.clone(),
            // path: dir_path.to_string_lossy().to_string(),
            manifest: scenario_manifest.clone(),
            modules: mods_to_load.clone(),
            // modules: matching_mods,
        })
    }
}
