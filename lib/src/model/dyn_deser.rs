//! Many objects within user-files are not statically defined,
//! rather a _Value_ object from serde is used. These _Value_ objects
//! are turned into proper objects based on some conditional logic.
//! This separation allows some fields to accept different data collections.
//! For example scenario manifest's mod dependencies can be either simple
//! 'name:version' or a more complex 'name:[version:version,git:address]',
//! which so far isn't possible with the default struct based serde.

use std::collections::HashMap;
use std::path::PathBuf;

use super::serde_yaml::{to_string, Value};
use regex::Regex;
use semver::VersionReq;

use {cmd, model, util};
use {Var, VarType};
use {
    DEFAULT_INACTIVE_STATE, DEFAULT_MODULE_DEP_VERSION, DEFAULT_SCENARIO_MODULE_DEP_VERSION,
    DEFAULT_SPAWN_COMPONENT_AT_INIT, DEFAULT_SPAWN_ENTITY_AT_INIT, DEFAULT_TRIGGER_EVENT,
};

use address::Address;
use cmd::{Command, ExtCommand};
use model::{
    CmdModel, ComponentModel, ComponentTypeModel, DataEntry, DataFileEntry, DataImageEntry,
    EntityModel, EntityTypeModel, ModuleDep, ScenarioModuleDep, VarModel,
};

impl EntityModel {
    /// Create entity model from a serde value representation.
    pub fn from_serde_value(
        entity_types: Vec<EntityTypeModel>,
        val: &Value,
    ) -> Option<EntityModel> {
        // str names of the fields
        let id_field = "id";
        let type_field = "type";
        let aliases_field = "aliases";
        let spawn_field = "spawn";

        // entity value has to be a mapping
        if !val.is_mapping() {
            //TODO print err
            return None;
        };

        // id is required
        let id: String = match val.get(id_field) {
            // id should be a string
            Some(v) => match v.as_str() {
                Some(s) => String::from(s),
                None => {
                    //                    let file_path = context.source_file.clone();
                    //                    context.err_min(format!("id entry on entity declaration is not a string, file: {:?} ({:?})",
                    //                                            file_path, val));
                    error!("id entry on entity declaration is not a string ({:?})", val);
                    return None;
                }
            },
            //TODO print err
            None => return None,
        };

        // we need the type of the entity
        let type_: String = match val.get(type_field) {
            Some(v) => match v.as_str().unwrap() {
                // universal entity is a special case because it's hardcoded
                t if t == "universal" => {
                    error!("can't declare universal entity, one is already provided by default");
                    return None;
                }
                t => {
                    let ts = String::from(t);
                    if !entity_types.iter().any(|entity_type| entity_type.id == ts) {
                        // entity_type doesn't exist
                        //TODO print better err
                        error!(
                            "entity_type \"{}\" was not declared, can't declare an entity of that type", ts);
                        return None;
                    }
                    // all good, entity type exists
                    ts
                } //                None => {
                  //                    println!("error: wrong entity type \"{}\" on entity declaration, file: {:?} ({:?}) ",
                  //                    v.as_str().unwrap(), context.file_path, val);
                  //                    return None;
                  //                }
            },
            None => {
                //                let file_path = context.source_file.clone();
                //                context.err_min(format!("error: type entry on entity declaration was not present, file: {:?} ({:?})",
                //                                        file_path, val));
                error!(
                    "error: type entry on entity declaration was not present ({:?})",
                    val
                );
                return None;
            }
        };

        let aliases: Vec<String> = match val.get(aliases_field) {
            Some(alias) => match alias.as_sequence() {
                Some(aseq) => {
                    let mut out_vec = Vec::new();
                    for v in aseq {
                        match v.as_str() {
                            Some(s) => out_vec.push(s.to_string()),
                            None => (),
                        }
                    }
                    out_vec
                }
                None => Vec::new(),
            },
            None => Vec::new(),
        };

        let spawn: bool = match val.get(spawn_field) {
            Some(spawn_val) => match spawn_val.as_bool() {
                Some(s) => s,
                None => DEFAULT_SPAWN_ENTITY_AT_INIT,
            },
            None => DEFAULT_SPAWN_ENTITY_AT_INIT,
        };

        Some(EntityModel {
            id,
            type_,
            aliases,
            spawn,
        })
    }
}

impl ComponentModel {
    /// Create component model from a serde value representation.
    pub fn from_serde_value(
        key: String,
        val: &Value,
        component_types: Vec<ComponentTypeModel>,
        module_path: &PathBuf,
    ) -> Result<ComponentModel, String> {
        // str names of the fields
        let id_field = "id";
        let type_field = "type";
        let entity_field = "entity";
        let vars_field = "vars";
        let states_field = "states";
        let start_state_field = "start";
        let triggers_field = "triggers";
        let aliases_field = "aliases";
        let spawn_field = "spawn";
        let scripts_field = "scripts";
        let libs_field = "libs";

        // component value has to be a mapping
        if !val.is_mapping() {
            return Err(format!("has to be a mapping"));
        };

        // id field is required
        let id: String = match val.get(id_field) {
            Some(v) => String::from(v.as_str().expect("failed as_str() on serde value")),
            None => {
                return Err(format!("id field is required"));
            }
        };

        // entity field is required
        let entity_type: String = match val.get(entity_field) {
            Some(v) => String::from(v.as_str().expect("failed as_str() on serde value")),
            None => return Err(format!("entity field is required")),
        };

        // component type has to match an already existing one
        // (this includes a match for component_type's entity_type
        let type_: String = match val.get(type_field) {
            Some(v) => match v.as_str() {
                Some(t) => {
                    let ts = String::from(t);
                    if !component_types.iter().any(|component_type| {
                        component_type.id == ts && component_type.entity_type == entity_type
                    }) {
                        // component_type doesn't exist
                        //TODO better err
                        return Err(format!(
                            "component_type \"{}\" for entity_type \"{}\" was not declared, \
                             can't declare a component of that type",
                            ts, entity_type
                        ));
                    }
                    // all good, component type exists
                    ts
                }
                None => {
                    return Err(format!("type has to be a string"));
                }
            },
            None => {
                return Err(format!("type field is required"));
            }
        };

        // vars field is optional
        let mut vars: Vec<VarModel> = Vec::new();
        match val.get(vars_field) {
            Some(v) => match VarModel::vec_from_serde_value(&v) {
                Ok(vec) => {
                    vars = vec;
                }
                Err(e) => return Err(format!("failed parsing vars field: {}", e)),
            },
            None => (),
        }

        // start state is optional, default is `none`
        let start_state: String = match val.get(start_state_field) {
            //            Some(v) => v.as_i64().unwrap() as u16,
            Some(v) => v.as_str().unwrap().to_string(),
            //                let s = v.as_str().unwrap();
            //                states.iter().enumerate()
            //                    .find(|(idx, state)| &state.id == s)
            //                    .map(|(idx, state)| idx as u16).unwrap()
            None => DEFAULT_INACTIVE_STATE.to_string(),
        };

        // trigger event is optional, default is `tick` which is the base tick
        let triggers: Vec<String> = match val.get(triggers_field) {
            Some(v) => match v.as_sequence() {
                Some(seq) => seq
                    .to_vec()
                    .iter()
                    .map(|value| value.as_str().unwrap_or("").to_string())
                    .collect(),
                None => return Err(format!("triggers field has to be a sequence")),
            },
            None => {
                trace!(
                    "declared component with no triggers\nmodule: {:?}\nval: {:?})",
                    module_path,
                    val
                );
                Vec::new()
            }
        };

        let aliases: Vec<String> = match val.get(aliases_field) {
            Some(alias) => match alias.as_sequence() {
                Some(aseq) => {
                    let mut out_vec = Vec::new();
                    for v in aseq {
                        match v.as_str() {
                            Some(s) => out_vec.push(s.to_string()),
                            None => (),
                        }
                    }
                    out_vec
                }
                None => Vec::new(),
            },
            None => Vec::new(),
        };

        let spawn: bool = match val.get(spawn_field) {
            Some(spawn_val) => match spawn_val.as_bool() {
                Some(s) => s,
                None => DEFAULT_SPAWN_COMPONENT_AT_INIT,
            },
            None => DEFAULT_SPAWN_COMPONENT_AT_INIT,
        };

        let script_files: Vec<PathBuf> = match val.get(scripts_field) {
            Some(scripts_val) => match scripts_val.as_sequence() {
                Some(scripts_seq) => {
                    let mut out_vec = Vec::new();
                    for script in scripts_seq {
                        if let Some(s) = script.as_str() {
                            let s_path = PathBuf::from(s);
                            out_vec.push(module_path.join(s_path));
                        }
                    }
                    //                    println!("{:?}", out_vec);
                    out_vec
                }
                None => Vec::new(),
            },
            None => Vec::new(),
        };

        let lib_files: Vec<PathBuf> = match val.get(libs_field) {
            Some(libs_val) => match libs_val.as_sequence() {
                Some(libs_seq) => {
                    let mut out_vec = Vec::new();
                    for lib in libs_seq {
                        if let Some(s) = lib.as_str() {
                            let extension = match ::TARGET_OS {
                                "linux" => "so",
                                "windows" => "dll",
                                _ => "",
                            };
                            let s_path = PathBuf::from(format!("{}.{}", s, extension));
                            out_vec.push(module_path.join(s_path));
                        }
                    }

                    out_vec
                }
                None => Vec::new(),
            },
            None => Vec::new(),
        };

        let mut component = ComponentModel {
            id,
            type_,
            entity_type,
            vars,
            states: Vec::new(),
            start_state,
            triggers,
            aliases,
            spawn,
            //            source_file: context.source_file.clone(),
            source_file: None,
            script_files,
            lib_files,
            model_uid: 0,
        };

        // states field is optional
        // always add a 'none' state (at index 0)
        //TODO should the 'none' state be added here or somewhere else?
        let mut states: Vec<StateModel> = vec![StateModel {
            id: "none".to_string(),
            loc: vec![],
            pre: vec![],
        }];
        match val.get(states_field) {
            Some(v) => {
                // states entry should be a sequence
                if !v.is_sequence() {
                    return Err(format!("`states` has to be a sequence"));
                }
                for state in v.as_sequence().unwrap() {
                    let state = StateModel::from_serde_value(state, &component)?;
                    states.push(state);
                }
            }
            None => (),
        }

        component.states = states;
        Ok(component)
    }
}

impl VarModel {
    fn new(key: &str, val: &Value) -> Result<VarModel, String> {
        // check if the key contains `internal`
        let mut internal = false;
        let mut split = key.split(" ").collect::<Vec<&str>>();
        if split.len() > 1 {
            if split[0] == "internal" {
                internal = true;
                split.remove(0);
            }
        }
        let s = split[0].split("/").collect::<Vec<&str>>();
        let type_ = VarType::from_str(s[0]).unwrap();

        //TODO add more types
        let default = match type_ {
            VarType::Str => match val {
                Value::String(v) => Var::Str(v.clone()),
                _ => return Err(format!("wrong type")),
            },
            VarType::Int => match val {
                Value::Number(v) => Var::Int(v.as_i64().unwrap() as i32),
                _ => return Err(format!("wrong type")),
            },
            VarType::Bool => match val {
                Value::Bool(v) => Var::Bool(*v),
                _ => return Err(format!("wrong type")),
            },
            VarType::StrList => {
                let mut out_grid = Vec::new();
                match val {
                    Value::Sequence(seq1) => {
                        for v in seq1 {
                            out_grid.push(v.as_str().unwrap().to_owned());
                        }
                        Var::StrList(out_grid)
                    }
                    Value::Null => Var::StrList(out_grid),
                    _ => return Err(format!("wrong type")),
                }
            }
            VarType::BoolList => {
                let mut out_list = Vec::new();
                match val {
                    Value::Sequence(seq1) => {
                        for v in seq1 {
                            out_list.push(v.as_bool().unwrap().to_owned());
                        }
                        Var::BoolList(out_list)
                    }
                    Value::Null => Var::BoolList(out_list),
                    _ => return Err(format!("wrong type")),
                }
            }
            VarType::IntGrid => {
                let mut out_grid = Vec::new();
                match val {
                    Value::Sequence(seq1) => {
                        let mut out_grid_intern = Vec::new();
                        for seq2 in seq1 {
                            if !seq2.is_sequence() {
                                return Err(format!("wrong type"));
                            } else {
                                for v in seq2.as_sequence().unwrap() {
                                    out_grid_intern.push(v.as_i64().unwrap() as i32);
                                }
                            }
                        }
                        out_grid.push(out_grid_intern);
                        Var::IntGrid(out_grid)
                    }
                    Value::Null => Var::IntGrid(out_grid),
                    _ => return Err(format!("wrong type")),
                }
            }
            VarType::StrGrid => {
                let mut out_grid = Vec::new();
                match val {
                    Value::Sequence(seq1) => {
                        let mut out_grid_intern = Vec::new();
                        for seq2 in seq1 {
                            if !seq2.is_sequence() {
                                return Err(format!("wrong type"));
                            } else {
                                for v in seq2.as_sequence().unwrap() {
                                    out_grid_intern.push(v.as_str().unwrap().to_owned());
                                }
                            }
                        }
                        out_grid.push(out_grid_intern);
                        Var::StrGrid(out_grid)
                    }
                    Value::Null => Var::StrGrid(out_grid),
                    _ => return Err(format!("wrong type")),
                }
            }
            _ => return Err(format!("wrong type")),
        };

        Ok(VarModel {
            id: s[1].to_string(),
            type_,
            default,
            internal,
        })
    }
    /// Create var models in bulk from a _sequence of vars_ serde value.
    fn vec_from_serde_value(value: &Value) -> Result<Vec<VarModel>, String> {
        // value has to be a sequence
        if !value.is_mapping() {
            return Err(format!("vars entry has to be a mapping"));
        }
        let map = value.as_mapping().unwrap();
        let mut out_vec: Vec<VarModel> = Vec::new();
        for (k, v) in map {
            match VarModel::new(k.as_str().unwrap(), v) {
                Ok(v) => out_vec.push(v),
                Err(e) => return Err(format!("failed parsing var: {}: ({:?}: {:?})", e, k, v)),
            }
            //            if var_value.is_mapping() {
            //                match Var::from_map(&var_value) {
            //                    Ok(v) => out_vec.push(v),
            //                    Err(e) => return Err(format!("failed parsing var (from map): {}", e)),
            //                }
            //            } else if var_value.is_string() {
            //                match Var::from_str(&var_value.as_str().unwrap()) {
            //                    Ok(v) => out_vec.push(v),
            //                    Err(e) => return Err(format!("failed parsing var (from string): {}", e)),
            //                }
            //            } else {
            //                return Err(format!("var has to be a mapping or a string"));
            //            }
        }
        Ok(out_vec)
    }
}

impl StateModel {
    /// Create state model from a serde value representation.
    pub fn from_serde_value(
        state_val: &Value,
        comp_model: &model::ComponentModel,
    ) -> Result<StateModel, String> {
        unimplemented!();
        // fields
        let id_field = "id";
        let loc_field = "loc";
        let pre_field = "pre";

        // id
        let id: String = match state_val.get(id_field) {
            Some(v) => {
                if v.is_string() {
                    v.as_str().unwrap().to_string()
                } else {
                    return Err(format!("id has to be a string"));
                }
            }
            None => {
                return Err(format!("id field is required"));
            }
        };

        // loc
        let mut loc_cmds: Vec<LocCommand> = Vec::new();
        match state_val.get(loc_field) {
            // field has to be a sequence
            Some(v) => {
                if v.is_sequence() {
                    // iterate commands
                    for cmd in v.as_sequence().unwrap() {
                        // cmd is a mapping
                        if cmd.is_mapping() {
                            // create a map of values that will be used to create the command object
                            let mut cmd_map: HashMap<String, Value> = HashMap::new();
                            for (ck, cv) in cmd.as_mapping().unwrap() {
                                cmd_map.insert(
                                    String::from(util::coerce_serde_val_to_string(ck)),
                                    cv.to_owned(),
                                );
                            }
                            loc_cmds.push(match cmd::Command::from_map(&cmd_map) {
                                Ok(c) => c,
                                Err(e) => {
                                    return Err(format!(
                                        "failed parsing loc command (from map): {}",
                                        e
                                    ));
                                }
                            });
                        }
                        // cmd is a string
                        else if cmd.is_string() {
                            let cmd =
                                match cmd::Command::from_str(&cmd.as_str().unwrap(), comp_model) {
                                    Ok(c) => c,
                                    Err(e) => {
                                        return Err(format!(
                                            "failed parsing loc command (from string): {}",
                                            e
                                        ));
                                    }
                                };
                            loc_cmds.push(cmd);
                        } else {
                            return Err(format!("command entry has to be a mapping or a string"));
                        }
                    }
                } else {
                    return Err(format!("commands field has to be a sequence"));
                }
            }
            _ => (),
        }

        // pre
        let mut pre_cmds: Vec<ExtCommand> = Vec::new();
        match state_val.get(pre_field) {
            // field has to be a sequence
            Some(v) => {
                if v.is_sequence() {
                    // iterate commands
                    for cmd in v.as_sequence().unwrap() {
                        //                    pre_cmds.push(cmd.clone());
                        //                    // cmd is a mapping
                        //                    if cmd.is_mapping() {
                        //                        // create a map of values that will be used to create the command object
                        //                        let mut cmd_map: HashMap<String, Value> = HashMap::new();
                        //                        for (ck, cv) in cmd.as_mapping().unwrap() {
                        //                            cmd_map.insert(
                        //                                String::from(util::coerce_serde_val_to_string(ck)),
                        //                                cv.to_owned());
                        //                        }
                        //                        pre_cmds.push(match cmd::ExtCommand::from_map(&cmd_map, ext_vars_ref) {
                        //                            Ok(c) => c,
                        //                            Err(e) => return Err(format!("failed parsing pre ext command (from map): {}", e)),
                        //                        });
                        //                    }
                        // cmd is a string
                        if cmd.is_string() {
                            let cmd_s = cmd.as_str().unwrap();
                            pre_cmds.push(match cmd::ExtCommand::from_str(cmd_s, comp_model) {
                                Ok(c) => c,
                                Err(e) => {
                                    return Err(format!(
                                        "failed parsing pre command (from string): {}",
                                        e
                                    ))
                                }
                            });
                        } else {
                            return Err(format!("pre command has to be a string"));
                        }
                    }
                } else {
                    return Err(format!("commands field has to be a sequence"));
                }
            }
            _ => (),
        }

        Ok(StateModel {
            id,
            loc: loc_cmds,
            pre: pre_cmds,
        })
    }
}

impl ModuleDep {
    /// Creates module dependency object from a serde value representation.
    pub fn from_serde_value(name: &String, value: &Value) -> ModuleDep {
        // str field names
        let version_field = "version";
        let git_field = "git";

        let mut version_req = String::from("*");
        let mut git_address = None;

        // simplest moduledep is a str version
        if let Some(s) = value.as_str() {
            match VersionReq::parse(s) {
                Ok(vr) => version_req = vr.to_string(),
                Err(e) => {
                    warn!(
                        "failed parsing module dep version req \"{}\", \
                         using default \"*\" ({})",
                        s, e
                    );
                }
            }
        }
        // otherwise it's a mapping with different kinds of entries
        else if let Some(mapping) = value.as_mapping() {
            unimplemented!();
            if let Ok(vr) = VersionReq::parse(value.as_str().unwrap()) {
                version_req = vr.to_string();
            } else {
                //TODO print warning about the version_req
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
        }

        ModuleDep {
            name: name.clone(),
            version_req,
            git_address,
        }
    }
}

impl ScenarioModuleDep {
    /// Creates scenario module dependency object from a serde value representation.
    pub fn from_serde_value(scenario_name: &String, value: &Value) -> Option<ScenarioModuleDep> {
        // str field names
        let version_field = "version";
        let git_field = "git";

        let mut version_req = String::from("*");
        let mut git_address = None;

        // simplest is a str version
        if let Some(s) = value.as_str() {
            match VersionReq::parse(s) {
                Ok(vr) => version_req = vr.to_string(),
                Err(e) => {
                    warn!(
                        "failed parsing scenario module dep version req \"{}\" ({}), \
                         using default \"*\" (any)",
                        s, e
                    );
                    version_req = VersionReq::any().to_string();
                }
            }
        }
        // otherwise it's a mapping with different kinds of entries
        else if let Some(mapping) = value.as_mapping() {
            unimplemented!();
            if let Ok(vr) = VersionReq::parse(value.as_str().unwrap()) {
                version_req = vr.to_string();
            } else {
                //TODO print warning about the version_req
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
        Some(ScenarioModuleDep {
            name: scenario_name.clone(),
            version_req,
            git_address,
        })
    }
}

impl DataEntry {
    pub fn list_from_serde_value(val: &Value) -> Option<Vec<DataEntry>> {
        if !val.is_mapping() {
            return None;
        }
        let valmap = val.as_mapping().unwrap();

        let mut out_vec = Vec::new();

        for (k, v) in valmap {
            // get k as str
            let mut ks: String = k.as_str().unwrap().to_string();
            // wildcard address
            if ks.contains("*") {
                if let Some(mapping) = v.as_mapping() {
                    // germany: 66
                    for (mk, mv) in mapping {
                        // clone the wildcard address
                        let mut lks = ks.clone();
                        // get the key as str
                        let mks = mk.as_str().unwrap();

                        let mksv: Vec<&str> = match mks.contains(",") {
                            true => mks.split(",").collect(),
                            false => vec![mks],
                        };
                        for mksp in mksv {
                            lks = lks.replacen("*", mksp, 1);
                        }
                        //                        println!("{}", lks);
                        if let Some(de) = DataEntry::single_from_parts(lks, mv) {
                            out_vec.push(de);
                        }
                    }
                } else {
                    return None;
                }
            }
            // regular address
            else {
                if let Some(de) = DataEntry::single_from_parts(ks, v) {
                    out_vec.push(de);
                }
            }
        }

        Some(out_vec)
    }
    fn single_from_parts(addr_str: String, v: &Value) -> Option<DataEntry> {
        //        use address::Address;
        //TODO this uses preset names for var types, won't work with aliases
        if addr_str.contains("/str/")
            || addr_str.contains("/int/")
            || addr_str.contains("/float/")
            || addr_str.contains("/bool/")
        {
            return Some(DataEntry::Simple((
                addr_str,
                util::coerce_serde_val_to_string(v),
            )));
        } else if addr_str.contains("/str_list/")
            || addr_str.contains("/int_list/")
            || addr_str.contains("/float_list/")
            || addr_str.contains("/bool_list/")
        {
            return Some(DataEntry::List((
                addr_str,
                v.as_sequence()
                    .unwrap()
                    .to_vec()
                    .iter()
                    .map(|v| util::coerce_serde_val_to_string(v))
                    .collect(),
            )));
        } else if addr_str.contains("/str_grid/")
            || addr_str.contains("/int_grid/")
            || addr_str.contains("/float_grid/")
            || addr_str.contains("/bool_grid/")
        {
            return Some(DataEntry::Grid((
                addr_str,
                v.as_sequence()
                    .unwrap()
                    .to_vec()
                    .iter()
                    .map(|v| {
                        v.as_sequence()
                            .unwrap()
                            .to_vec()
                            .iter()
                            .map(|vv| util::coerce_serde_val_to_string(vv))
                            .collect()
                    })
                    .collect(),
            )));
        } else {
            return None;
        }
    }
}

impl DataImageEntry {
    pub fn list_from_serde_value(val: &Value, module_path: PathBuf) -> Vec<DataImageEntry> {
        if !val.is_mapping() {
            return Vec::new();
        }
        let valmap = val.as_mapping().unwrap();
        let mut out_vec = Vec::new();
        for (k, v) in valmap {
            // get k as str
            let mut ks: String = k.as_str().unwrap().to_string();

            // regular address
            if let Some(de) = DataImageEntry::single_from_parts(ks, v, module_path.clone()) {
                out_vec.push(de);
            }
        }
        out_vec
    }
    fn single_from_parts(
        addr_str: String,
        val: &Value,
        module_path: PathBuf,
    ) -> Option<DataImageEntry> {
        if !val.is_mapping() {
            return None;
        }
        let mut type_ = String::new();
        let mut path = String::new();

        let valmap = val.as_mapping().unwrap();
        for (k, v) in valmap {
            let ks: String = k.as_str().unwrap().to_string();
            let vs: String = v.as_str().unwrap().to_string();
            match ks.as_str() {
                "type" => type_ = vs,
                "path" => path = module_path.join(vs).to_str().unwrap().to_string(),
                _ => (),
            }
        }
        //TODO this uses preset names for var types, won't work with aliases
        if addr_str.contains("/int_grid/") {
            match type_.as_str() {
                "bmp_u8" => return Some(DataImageEntry::BmpU8(addr_str, path)),
                "png_u8u8u8_concat" => {
                    return Some(DataImageEntry::PngU8U8U8Concat(addr_str, path))
                }
                "png_u8u8u8" => return Some(DataImageEntry::PngU8U8U8(addr_str, path)),
                _ => return None,
            }
        }

        None
    }
}
