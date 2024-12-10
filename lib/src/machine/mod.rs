//! `machine` denotes a [`processor`] task set up to act as a virtual machine
//! executing a proprietary instruction set.
//!
//! The name can also be taken as hinting at the *(finite) state machine*
//! functionality the `machine` construct provides.
//!
//! Because `machine` is only able to process predefined instructions it's
//! great for sandboxing.
//!
//!
//! # Instancing
//!
//! `machine` is provided alongside an instancing mechanism, the same that's
//! used for [`processor`]. It allows for automatically spawning `machine`s
//! based on preselected requirements. For example we could spawn the same
//! piece of logic for each entity that has some specific component attached.
//!
//!
//! [`processor`]: crate::processor

pub mod cmd;
pub mod error;
pub mod exec;
pub mod script;

pub use cmd::Command;
pub use error::{Error, ErrorKind, Result};

use std::collections::BTreeMap;

use arrayvec::ArrayVec;
use smallvec::SmallVec;
use tokio::runtime;
use tokio_stream::StreamExt;

use crate::address::LocalAddress;
use crate::entity::StorageIndex;
use crate::executor::LocalExec;
use crate::processor::ProcessorHandle;
use crate::query::{Query, QueryProduct};
use crate::string;
use crate::{
    rpc, CompName, EntityId, EntityName, Executor, LongString, ShortString, StringId, VarType,
};

pub const START_STATE_NAME: &'static str = "start";

#[cfg(feature = "machine_dynlib")]
pub type Libraries = BTreeMap<String, Library>;
#[cfg(feature = "machine_dynlib")]
use libloading::Library;

pub struct Machine {
    state: StringId,
    worker: LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
}

impl Machine {
    pub async fn query(&self, query: Query) -> Result<String> {
        let response = self
            .worker
            .execute(rpc::worker::Request::Query(query))
            .await?;
        if let Ok(rpc::worker::Response::Query(product)) = response {
            if let QueryProduct::AddressedVar(map) = product {
                let (addr, var) = map.iter().next().unwrap();
                return Ok(var.to_string());
            } else {
                unimplemented!()
            }
        } else {
            unimplemented!()
        }
    }
}

#[derive(Clone)]
pub struct MachineHandle {
    executor: LocalExec<rpc::machine::Request, Result<rpc::machine::Response>>,
    processor: ProcessorHandle,
}

#[async_trait::async_trait]
impl Executor<rpc::machine::Request, Result<rpc::machine::Response>> for MachineHandle {
    async fn execute(
        &self,
        req: rpc::machine::Request,
    ) -> crate::error::Result<Result<rpc::machine::Response>> {
        self.executor.execute(req).await
    }
}

impl MachineHandle {
    pub async fn execute_cmd(&self, cmd: cmd::Command) -> Result<rpc::machine::Response> {
        self.execute(rpc::machine::Request::Execute(cmd)).await?
    }

    pub async fn execute_cmd_batch(
        &self,
        cmds: Vec<cmd::Command>,
    ) -> Result<rpc::machine::Response> {
        self.execute(rpc::machine::Request::ExecuteBatch(cmds))
            .await?
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.processor
            .execute(rpc::processor::Request::Shutdown)
            .await
            .map_err(|e| Error::new(LocationInfo::empty(), ErrorKind::CoreError(e.to_string())))
            .map(|_| ())
    }
}

/// Spawns a new machine processor task.
///
/// Returns a handle that can be used to communicate with the machine from the
/// outside.
///
/// The handle supports operations specific to machine operations as opposed to
/// regular processors. Internally the machine task uses the generic processor
/// executor with it's standard request/response types.
pub fn spawn(
    worker: LocalExec<rpc::worker::Request, crate::Result<rpc::worker::Response>>,
    runtime: runtime::Handle,
) -> Result<MachineHandle> {
    let (mut sender, receiver) = tokio::sync::mpsc::channel::<(
        rpc::machine::Request,
        tokio::sync::oneshot::Sender<Result<rpc::machine::Response>>,
    )>(20);
    let mut stream = tokio_stream::wrappers::ReceiverStream::new(receiver);
    let mut executor = LocalExec::new(sender);

    let f = |mut processor_stream: tokio_stream::wrappers::ReceiverStream<(
        rpc::processor::Request,
        tokio::sync::oneshot::Sender<crate::Result<rpc::processor::Response>>,
    )>,
             worker| async move {
        let mut machine = Machine {
            state: string::new_truncate("start"),
            worker,
        };

        loop {
            tokio::select! {
                Some((request, send)) = stream.next() => {
                    match request {
                        rpc::machine::Request::Execute(cmd) => {
                            // unimplemented!();
                            trace!("executing cmd: {cmd:?}");
                            let result = cmd
                                .execute(
                                    &mut machine,
                                    &mut "".to_string(),
                                    &mut ArrayVec::new(),
                                    &mut Registry {
                                        str0: LongString::new(),
                                        int0: 0,
                                        float0: 0.,
                                        bool0: false,
                                    },
                                    &"".to_string(),
                                    &0,
                                    &LocationInfo::empty(),
                                )
                                .await;
                            send.send(Ok(rpc::machine::Response::Empty));
                            // cmd.execute();
                        }
                        _ => unimplemented!(),
                    }
                }
                Some((request, send)) = processor_stream.next() => {
                    match request {
                        rpc::processor::Request::Shutdown =>  {
                            send.send(Ok(rpc::processor::Response::Empty));
                            return Ok(());
                        }
                        _ => unimplemented!(),
                    }
                }
                else => {
                    continue;
                }

            }
        }
        Ok(())
    };

    let processor = crate::processor::spawn(f, worker, runtime)?;

    Ok(MachineHandle {
        executor,
        processor,
    })
}

/// Holds instruction location information.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct LocationInfo {
    /// Path to project root
    pub root: Option<LongString>,
    /// Path to the source file, relative to project root
    pub source: Option<LongString>,
    /// Line number as seen in source file
    pub source_line: Option<usize>,
    /// Line number after trimming empty lines, more like an command index
    pub line: Option<usize>,
    /// Unique tag for this location
    pub tag: Option<StringId>,

    pub comp_name: Option<StringId>,
}

impl LocationInfo {
    pub fn to_string(&self) -> String {
        format!(
            "Source: {}, Line: {}",
            self.source
                .as_ref()
                .unwrap_or(&LongString::from("unknown").unwrap()),
            self.source_line.as_ref().unwrap_or(&0)
        )
    }
    pub fn empty() -> LocationInfo {
        LocationInfo {
            root: None,
            source: None,
            source_line: None,
            line: None,
            tag: None,
            comp_name: None,
        }
    }

    pub fn with_source(mut self, root: &str, source: &str) -> Self {
        self.root = Some(root.parse().unwrap());
        self.source = Some(source.parse().unwrap());
        self
    }
}

/// Command in it's simplest form, ready to be turned into a more concrete
/// representation.
///
/// Example:
/// ```text
/// command_name --flag --arg="something" -> output:address
/// ```
#[derive(Debug, Clone)]
pub struct CommandPrototype {
    /// Command name
    pub name: Option<String>,
    /// Command arguments
    pub arguments: Option<Vec<String>>,
    /// Command output
    pub output: Option<String>,
}

/// Custom collection type used as the main call stack during logic execution.
//TODO determine optimal size, determine whether it should be fixed size or not
pub(crate) type CallStackVec = ArrayVec<CallInfo, 32>;

/// Collection type used to hold command results.
pub(crate) type CommandResultVec = SmallVec<[cmd::CommandResult; 2]>;

/// Struct containing basic information about where the execution is
/// taking place.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct ExecutionContext {
    pub ent: EntityId,
    pub comp: CompName,
    pub location: LocationInfo,
}

/// List of "stack" variables available only to the component machine
/// and not visible from the outside.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Registry {
    pub str0: LongString,
    pub int0: i32,
    pub float0: f32,
    pub bool0: bool,
}
impl Registry {
    pub fn new() -> Registry {
        Registry {
            str0: LongString::new(),
            int0: 0,
            float0: 0.0,
            bool0: false,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RegistryTarget {
    Str0,
    Int0,
    Float0,
    Bool0,
}

/// Information about a single call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub enum CallInfo {
    Procedure(ProcedureCallInfo),
    ForIn(ForInCallInfo),
    Loop(LoopCallInfo),
    IfElse(IfElseCallInfo),

    Component(ComponentCallInfo),
}

/// Information about a single procedure call.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProcedureCallInfo {
    pub call_line: usize,
    pub start_line: usize,
    pub end_line: usize,
    // pub output_variable: Option<String>,
}

/// Information about a single forin call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct ForInCallInfo {
    /// Target that will be iterated over
    pub target: Option<LocalAddress>,
    pub target_len: usize,
    /// Variable to update while iterating
    pub variable: Option<LocalAddress>,
    // pub variable_type: Option<VarType>,
    /// Current iteration
    pub iteration: usize,
    // meta
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LoopCallInfo {
    start: usize,
    end: usize,
}

/// Contains information about a single ifelse call.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IfElseCallInfo {
    pub current: usize,
    pub passed: bool,
    pub else_line_index: usize,
    pub meta: IfElseMetaData,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IfElseMetaData {
    pub start: usize,
    pub end: usize,
    pub else_lines: [usize; 10],
}

/// Contains information about a single component block call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "small_stringid", derive(Copy))]
pub struct ComponentCallInfo {
    pub name: CompName,
    pub start_line: usize,
    pub end_line: usize,
    // pub current: usize,
    // pub passed: bool,
    // pub else_line_index: usize,
    // pub meta: IfElseMetaData,
}

/// Performs a command search on the provided command prototype list.
///
/// Goal is to find the end, and potentially intermediate parts, of a block.
/// To accomplish this, the function takes lists of defs describing beginning,
/// middle and ending marks of any blocks that it may stumble upon during
/// the search.
///
/// On success returns a tuple of single end part line numer and list of middle
/// part line numbers. If no matching parts are found, and no error .
pub(crate) fn command_search(
    location: &LocationInfo,
    commands: &Vec<CommandPrototype>,
    constraints: (usize, Option<usize>),
    defs: (&Vec<&str>, &Vec<&str>, &Vec<&str>),
    blocks: (&Vec<&str>, &Vec<&str>),
    recurse: bool,
) -> Result<Option<(usize, Vec<usize>)>> {
    if defs.0.is_empty() {
        return Err(Error::new(
            location.clone(),
            ErrorKind::CommandSearchFailed(
                "command search requires begin definitions to be non-empty".to_string(),
            ),
        ));
    }
    if defs.2.is_empty() {
        return Err(Error::new(
            location.clone(),
            ErrorKind::CommandSearchFailed(
                "command search requires ending definitions to be non-empty".to_string(),
            ),
        ));
    }
    let mut locs = (0, Vec::new());
    let mut skip_to = constraints.0;
    let mut block_diff = 0;
    let finish_idx = commands.len();
    for line in constraints.0..finish_idx {
        if line >= skip_to {
            let command = &commands[line];
            match &command.name {
                Some(command) => {
                    if blocks.0.contains(&command.as_str()) {
                        block_diff = block_diff + 1;
                    } else if defs.1.contains(&command.as_str()) {
                        locs.1.push(line);
                    } else if blocks.1.contains(&command.as_str()) && block_diff > 0 {
                        block_diff = block_diff - 1;
                    } else if defs.2.contains(&command.as_str()) {
                        locs.0 = line;
                        return Ok(Some(locs));
                    } else if defs.0.contains(&command.as_str()) {
                        if recurse {
                            match command_search(
                                location,
                                commands,
                                (line + 1, Some(finish_idx)),
                                defs,
                                blocks,
                                recurse,
                            ) {
                                Ok(locs_opt) => match locs_opt {
                                    Some(_locs) => {
                                        skip_to = _locs.0 + 1;
                                        ()
                                    }
                                    None => {
                                        return Err(Error::new(
                                            location.clone(),
                                            ErrorKind::CommandSearchFailed(format!(
                                                "bad nesting: got {} but end not found",
                                                command
                                            )),
                                        ))
                                    }
                                },
                                Err(error) => return Err(error),
                            };
                        } else {
                            return Err(Error::new(
                                location.clone(),
                                ErrorKind::CommandSearchFailed(format!(
                                    "bad nesting: got {}",
                                    command,
                                )),
                            ));
                        }
                    }
                    ()
                }
                None => (),
            }
        }
    }

    Err(Error::new(
        location.clone(),
        ErrorKind::CommandSearchFailed(format!(
            "no end of structure for begin defs: {:?}",
            &defs.0
        )),
    ))
}

pub enum SpawnRequirement {
    PerEntityWithAllComponents(Vec<CompName>),
    PerEntityWithAnyComponent(Vec<CompName>),
    PerWorker,
}
