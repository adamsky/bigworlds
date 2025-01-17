#[derive(Clone, Debug)]
pub struct Config {
    pub addr: Option<String>,

    /// Topology strategy for the worker to follow
    pub topo_strategy: TopoStrategy,

    // TODO consider max CPU usage and how it could be calculated
    /// Maximum allowed memory use
    pub max_ram_mb: usize,
    /// Maximum allowed disk use
    pub max_disk_mb: usize,
    /// Maximum allowed network transfer use
    pub max_transfer_mb: usize,

    // TODO overhaul the authentication system
    /// Whether the worker uses a password to authorize connecting comrade
    /// workers
    pub use_auth: bool,
    /// Password used for incoming connection authorization
    pub passwd_list: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: None,
            topo_strategy: Default::default(),
            max_ram_mb: 0,
            max_disk_mb: 0,
            max_transfer_mb: 0,
            use_auth: false,
            passwd_list: vec![],
        }
    }
}

/// Strategy the worker will pursue in terms of connecting directly to other
/// workers.
///
/// Worker gets information about how to connect to other workers from the
/// leader.
///
/// All strategies involve worker continuously working to upheld the defined
/// goal. For example connecting to all means also connecting to new workers
/// as they join the cluster. As another example, when the strategy is to only
/// connect to a single remote worker and that remote worker dies, the worker
/// needs to connect to another remote worker.
#[derive(Clone, Debug, Default)]
pub enum TopoStrategy {
    /// Maintain connection to only one remote worker
    ConnectToOne,
    /// Maintain connections with `n` remote workers selected at random
    ConnectToRandom(u8),
    /// Maintain connections with `n` closest remote workers, selected by ping
    ConnectToClosest(u8),
    /// Maintain connections to all workers in the cluster
    #[default]
    ConnectToAll,
}
