extern crate fasteval;
extern crate getopts;

use std::collections::{BTreeMap, HashMap};
use std::process::Command as ProcessCommand;

// use evalexpr::eval;
use fasteval::{Compiler, Evaler};

use crate::address::{Address, LocalAddress, ShortLocalAddress};
use crate::{string, CompName, Float, StringId, Var, VarType};

// use serde_yaml::Value;
// use shlex::split;
//
use self::getopts::Options;

// use crate::component::Component;
use crate::entity::{Entity, Storage};
// use crate::error::Error;
use crate::model::Model;

use super::super::{CommandPrototype, Error, LocationInfo, Registry, RegistryTarget, Result};
use super::{Command, CommandResult};
use crate::machine::ErrorKind;
use std::str::FromStr;

/// Precompiles an evaluation and stores it
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eval {
    pub expr: fasteval::Instruction,
    pub slab: fasteval::Slab,
    pub args: Vec<(StringId, ShortLocalAddress)>,
    // pub arg0: Option<(ShortString, RegistryTarget)>,
    pub out: Option<ShortLocalAddress>,
}

impl Eval {
    pub fn new(args: Vec<String>) -> Result<Command> {
        let matches = getopts::Options::new()
            .optopt("o", "out", "", "")
            .parse(&args)?;

        let mut slab = fasteval::Slab::new();
        let parser = fasteval::Parser::new();
        let compiled = parser
            .parse(&matches.free[0], &mut slab.ps)
            .unwrap()
            .from(&slab.ps)
            .compile(&slab.ps, &mut slab.cs);

        // let mut out = None;
        let out = matches
            .opt_str("out")
            .map(|s| ShortLocalAddress::from_str(&s))
            .transpose()?;

        let mut eval_args = Vec::new();
        for free_arg in matches.free.iter().skip(1) {
            let split = free_arg.split('=').collect::<Vec<&str>>();
            if split.len() == 2 {
                eval_args.push((
                    string::new_truncate(split[0]),
                    ShortLocalAddress::from_str(&split[1])?,
                ));
            }
        }

        Ok(Command::Eval(Eval {
            expr: compiled,
            slab,
            args: eval_args,
            out,
        }))
    }
    pub fn execute_loc(
        &self,
        storage: &mut Storage,
        comp_name: &CompName,
        registry: &mut Registry,
        location: &LocationInfo,
    ) -> CommandResult {
        let mut ns = fasteval::StringToF64Namespace::new();
        // let mut map = BTreeMap::new();
        for (arg_name, arg_addr) in &self.args {
            let val = match storage.get_var(&arg_addr.storage_index_using(comp_name.clone())) {
                Ok(v) => v.to_float(),
                Err(e) => {
                    return CommandResult::Err(Error::new(
                        location.clone(),
                        ErrorKind::CoreError(e.to_string()),
                    ));
                }
            };
            // println!("position:x value: {}", xval);
            ns.insert(arg_name.to_string(), val as f64);
        }

        // let val = fasteval::ez_eval(&self.expr, &mut ns).unwrap();
        let val = self.expr.eval(&self.slab, &mut ns).unwrap();
        // let val = fasteval::eval_compiled!(self.expr, &self.slab, &mut ns);
        // println!("evaled val: {}", val);

        if let Some(out) = &self.out {
            let mut target = storage
                .get_var_mut(&out.storage_index_using(comp_name.clone()))
                .unwrap();
            // *target = crate::Var::fr
            // let v = crate::Var::Float(val as crate::Float);
            // println!("newly created var::float: {:?}", v);
            *target = Var::Float(val as Float);
        }

        // match self.out {
        //     RegistryTarget::Str0 => registry.str0 =
        // ShortString::from_str_truncate(format!("{}", val)),     _ => (),
        // }

        // println!("eval result: {}", val);
        CommandResult::Continue
    }
}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct EvalReg {
//     pub expr: String,
//     pub arg0: Option<(StringId, RegistryTarget)>,
//     pub out: RegistryTarget,
// }
// impl EvalReg {
//     pub fn new(
//         args: Vec<String>,
//         location: &LocationInfo,
//         commands: &Vec<CommandPrototype>,
//     ) -> Result<Command> {
//         let cmd = EvalReg {
//             expr: args[0].to_string(),
//             arg0: None,
//             out: RegistryTarget::Str0,
//         };
//         Ok(Command::EvalReg(cmd))
//     }
//     pub fn execute_loc(&self, registry: &mut Registry) -> CommandResult {
//         // let mut ns = fasteval::EmptyNamespace;
//         // let mut slab = fasteval::Slab::new();
//         // // let val = fasteval::ez_eval(&self.expr, &mut ns).unwrap();
//         // let val = precomps[0].eval(&slab, &mut ns).unwrap();
//         // match self.out {
//         //     RegistryTarget::Str0 => registry.str0 =
// ShortString::from_str_truncate(format!("{}", val)),         //     _ => (),
//         // }
//         //
//         // println!("eval result: {}", val);
//         CommandResult::Continue
//     }
// }

// #[derive(Debug, Serialize, Deserialize, Clone)]
// pub struct EvalOld {
//     pub expr: String,
//     pub args: HashMap<String, Address>,
//     pub false_: CommandResult,
//     pub true_: CommandResult,
//     pub out: Option<Address>,
// }
// impl EvalOld {
//     pub fn from_str(mut args_str: &str) -> Result<Self> {
//         let mut out = None;
//         if args_str.contains("|") {
//             let split: Vec<&str> = args_str.splitn(2,
// "|").collect::<Vec<&str>>();             //            println!("{:?}",
// split);             args_str = split[0].trim();
//             let pipe_addr = Address::from_str_with_context(split[1].trim(),
// None, None).unwrap();             out = Some(pipe_addr);
//         }
//         let shl_split = match shlex::split(args_str) {
//             Some(s) => s,
//             None => return Err(format!("failed parsing command arguments:
// {}", args_str)),         };
//         //        println!("{:?}", &shl_split);
//         let mut cr_false = CommandResult::Break;
//         let mut cr_true = CommandResult::Continue;
//         let mut options = Options::new();
//         options.optopt(
//             "f",
//             "false",
//             "Result triggered if evaluates to false.",
//             "RESULT",
//         );
//         options.optopt(
//             "t",
//             "true",
//             "Result triggered if evaluates to true.",
//             "RESULT",
//         );
//         options.optopt("o", "out", "Output destination.", "RESULT");
//         //        println!("{}", options.short_usage("equal"));
//         let opt_res = options.parse(&shl_split[1..]).unwrap();
//         if let Some(s) = opt_res.opt_str("false") {
//             cr_false = CommandResult::from_str(&s).unwrap();
//         }
//         if let Some(s) = opt_res.opt_str("true") {
//             cr_true = CommandResult::from_str(&s).unwrap();
//         }
//         if let Some(s) = opt_res.opt_str("out") {
//             out = Address::from_str_with_context(&s, None, None)
//         }
//         unimplemented!();
//         // let regex = Regex::new(r#"\{\{(.*?)\}\}"#).expect("failed creating
// new regex");         // let mut split: Vec<String> = regex
//         //     .captures_iter(args_str)
//         //     .into_iter()
//         //     .map(|s| s[0].to_string())
//         //     .collect();
//         // split = split.iter_mut().map(|s| s.to_string()).collect();
//         // //        println!("{:?}", split);
//         // let mut args = HashMap::new();
//         // for match_ in &split {
//         //     args.insert(
//         //         match_.clone(),
//         //         Address::from_str_with_context(
//         //             &match_.replace("{{", "").replace("}}", ""),
//         //             None,
//         //             None,
//         //         )
//         //         .unwrap(),
//         //     );
//         // }
//         //
//         // Ok(EvalOld {
//         //     //            expr: args_str.to_string(),
//         //     expr: shl_split[0].to_string(),
//         //     args,
//         //     false_: cr_false,
//         //     true_: cr_true,
//         //     out: out,
//         // })
//     }
// }
//
// impl EvalOld {
//     pub fn execute_loc(&self, ent_storage: &mut Storage) -> CommandResult {
//         let mut ns = fasteval::EmptyNamespace;
//         let val = fasteval::ez_eval(&self.expr, &mut ns).unwrap();
//
//         //        debug!("execute loc eval");
//         let mut expr = self.expr.clone();
//         for (arg, addr) in &self.args {
//             //            let ev = comp.loc_vars.get(*addr).unwrap();
//             //            println!("{:?}", addr);
//             let stri = match ent_storage.get_coerce_to_string(addr, None) {
//                 Some(s) => s,
//                 None => return self.false_.clone(),
//             };
//             expr = expr.replace(arg, &stri);
//         }
//         // let res = match eval(&expr) {
//         //     Ok(v) => v,
//         //     Err(e) => {
//         //         error!("{}", e);
//         //         return self.false_.clone();
//         //     }
//         // };
//         unimplemented!();
//         // match self.out {
//         //     Some(addr) => {
//         //         //                let ev = comp.loc_vars.get(v).unwrap();
//         //         match addr.get_var_type() {
//         //             VarType::Str => match res.as_string() {
//         //                 Ok(v) =>
// *ent_storage.get_str_mut(&addr.get_storage_index()).unwrap() = v,         //
// Err(e) => error!("pipe failed: {}", e),         //             },
//         //             VarType::Int => match res.as_int() {
//         //                 Ok(v) => {
//         //
// *ent_storage.get_int_mut(&addr.get_storage_index()).unwrap() = v as i32
//         //                 }
//         //                 Err(e) => error!("pipe failed: {}", e),
//         //             },
//         //             VarType::Float => match res.as_number() {
//         //                 Ok(v) => {
//         //                     *ent_storage
//         //                         .get_float_mut(&addr.get_storage_index())
//         //                         .unwrap() = v as f32
//         //                 }
//         //                 Err(e) => error!("pipe failed: {}", e),
//         //             },
//         //             VarType::Bool => match res.as_boolean() {
//         //                 Ok(v) =>
// *ent_storage.get_bool_mut(&addr.get_storage_index()).unwrap() = v,         //
// Err(e) => error!("pipe failed: {}", e),         //             },
//         //             t => error!(
//         //                 "`eval` cmd pipeout doesn't support the following
// type: {:?}",         //                 t
//         //             ),
//         //         };
//         //     }
//         //     None => (),
//         // }
//         // match res {
//         //     evalexpr::Value::Boolean(b) => match b {
//         //         false => return self.false_.clone(),
//         //         true => return self.true_.clone(),
//         //     },
//         //     _ => return self.true_.clone(),
//         // }
//         unimplemented!()
//     }
// }
