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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_server_and_client_ticks_have_a_side() {
        assert_eq!(TickKind::Server.side(), Some(Side::Server));
        assert_eq!(TickKind::Client.side(), Some(Side::Client));
        assert_eq!(TickKind::World.side(), None);
        assert_eq!(TickKind::Entity.side(), None);
    }

    #[test]
    fn error_codes_are_unique() {
        let codes = [
            TickError::Failed(String::new()).code(),
            TickError::from(SideError::Unavailable).code(),
        ];
        assert_eq!(codes, [1, 2]);
    }
}
