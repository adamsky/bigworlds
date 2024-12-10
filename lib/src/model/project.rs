use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Display;
use std::io::{Read, Write};
use std::path::PathBuf;

use bytes::Buf;
use vfs::FileSystem;

use crate::model::deser;
use crate::model::module::Module;
use crate::model::scenario::Scenario;
use crate::{
    util, Error, Model, Result, MODULES_DIR_NAME, PROJECT_MANIFEST_FILE, SCENARIOS_DIR_NAME,
};

/// Top-level model structure describing project contents.
///
/// # Self-contained
///
/// Project is self-contained and meant to be serialized and sent over the
/// wire.
///
/// It abstracts away most of the file-related structure of a project. This is
/// not possible in some cases, such as when dealing with embedded service code
/// meant to be compiled on target machine.
///
/// # Extracting runnable model
///
/// Project itself cannot be fed into the simulation engine. Proper model must
/// first be extracted. Extraction here means selecting one of the scenarios
/// available in the project, which then also defines what modules are loaded
/// and in what order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub scenarios: Vec<Scenario>,
    pub modules: Vec<Module>,
}

impl Project {
    /// Creates a new `Project` from provided tar archive.
    ///
    /// # Virtual FS
    ///
    /// Using the `vfs` crate we're able to unpack archive into a in-memory
    /// filesystem structure and then pass it on to `Project::from_files`.
    pub fn from_archive(bytes: Vec<u8>) -> Result<Self> {
        let bytes = bytes::Bytes::from(bytes);
        let mut archive = tar::Archive::new(bytes.reader());
        let mut entries = archive.entries().unwrap();

        let mut mfs = vfs::MemoryFS::new();

        // go over all archive entries and recreate the file structure on the
        // in-memory filesystem
        while let Some(f) = entries.next() {
            let mut archive_file = f.unwrap();
            let mut fs_file = mfs
                .create_file(archive_file.path().unwrap().to_str().unwrap())
                .unwrap();
            let mut data = Vec::new();
            archive_file.read_to_end(&mut data);
            fs_file.write_all(&data).unwrap();
        }

        // we don't provide a root here since the filesystem is created at
        // archive root, which we assume is the project root
        Self::from_files(&mfs, None)
    }

    /// Creates a new `Project` from provided filesystem. Optionally a root
    /// path to project directory can be provided.
    ///
    /// # Virtual FS
    ///
    /// The use of generic `vfs::FileSystem` allows this function to take in
    /// both abstraction over regular filesystem, including alt-root variant,
    /// and in-memory filesystem.
    pub fn from_files<FS: vfs::FileSystem>(fs: &FS, root: Option<&str>) -> Result<Self> {
        let manifest = ProjectManifest::from_dir_at(fs, root.unwrap_or(""))?;

        // go through modules and process them one by one
        let modules_dir = format!("{}/{}", root.unwrap_or(""), MODULES_DIR_NAME);
        trace!("modules_dir: {}", modules_dir);
        let mut modules = Vec::new();
        let module_dirs = fs.read_dir(&modules_dir)?;
        for module_dir in module_dirs {
            let module_root = format!("{}/{}", modules_dir, module_dir);
            trace!("module_root: {}", module_root);
            let module = Module::from_files(fs, Some(module_root.as_str()))?;
            modules.push(module);
        }

        // go through scenarios
        let scenarios_dir = format!("{}/{}", root.unwrap_or(""), SCENARIOS_DIR_NAME);
        trace!("scenarios_dir: {}", scenarios_dir);
        let mut scenarios = Vec::new();
        let scenario_entries = fs.read_dir(&scenarios_dir)?;
        for scenario_entry in scenario_entries {
            let scenario_path = format!("{}/{}", scenarios_dir, scenario_entry);
            let scenario = Scenario::from_path(fs, &scenario_path)?;
            scenarios.push(scenario);
        }

        Ok(Self {
            name: manifest.name,
            scenarios,
            modules,
        })
    }

    /// Extracts runnable simulation model from the project using provided
    /// scenario name.
    pub fn extract_model(&self, scenario: &str) -> Result<Model> {
        let scenario = self
            .scenarios
            .iter()
            .find(|s| s.name == scenario)
            .ok_or(Error::Other(format!(
                "Scenario not found in project: {}",
                scenario
            )))?;
        let mut model = Model::default();
        // println!("{:?}", self.modules);
        for module_req in &scenario.modules {
            // println!("{:?}", module_req);
            let module = self
                .modules
                .iter()
                .find(|m| m.manifest.name == module_req.name)
                // .find(|m| m.manifest.version == module_req.version_req)
                .ok_or(Error::ScenarioMissingModule(module_req.name.clone()))?
                .clone();
            model.merge(module.model)?;
        }
        Ok(model)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectManifest {
    // required
    pub name: String,

    // optional
    /// Module description
    pub description: Option<String>,
    /// Author information
    pub author: Option<String>,
    /// Website information
    pub website: Option<String>,
}

impl ProjectManifest {
    /// Create module manifest from path to module directory
    pub fn from_dir_at<FS: vfs::FileSystem>(fs: &FS, dir_path: &str) -> Result<Self> {
        let manifest_path = format!("{}/{}", dir_path, PROJECT_MANIFEST_FILE);
        let deser: deser::ProjectManifest = util::deser_struct_from_path(fs, &manifest_path)?;

        Ok(Self {
            name: deser.project.name,
            description: deser.project.description,
            author: deser.project.author,
            website: deser.project.website,
        })
    }
}
