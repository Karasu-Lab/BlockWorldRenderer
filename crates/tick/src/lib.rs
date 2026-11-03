use thiserror::Error;
use utils::side::{Side, SideError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Tick(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TickKind {
    World,
    Entity,
    Server,
    Client,
}

impl TickKind {
    pub fn side(self) -> Option<Side> {
        match self {
            TickKind::Server => Some(Side::Server),
            TickKind::Client => Some(Side::Client),
            TickKind::World | TickKind::Entity => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TickError {
    #[error("tick failed: {0}")]
    Failed(String),
    #[error(transparent)]
    Side(#[from] SideError),
}

impl TickError {
    pub fn code(&self) -> u16 {
        match self {
            TickError::Failed(_) => 1,
            TickError::Side(_) => 2,
        }
    }
}

pub trait Tickable {
    fn tick(&mut self, tick: Tick);
}

pub trait Ticker {
    fn kind(&self) -> TickKind;

    fn run(&mut self, tick: Tick) -> Result<(), TickError>;
}

pub trait TickScheduler {
    fn current(&self) -> Tick;

    fn schedule(&mut self, delay: u64, task: Box<dyn FnOnce(Tick) + Send>);

    fn advance(&mut self) -> Tick;
}
