use bwr_entity::{Entity, EntityId};
use bwr_network::{Connection, NetworkError};
use bwr_registry::ResourceKey;
use bwr_storage::ChunkStorage;
use bwr_worldgen::ChunkGenerator;
use bwr_worldtick::Tick;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSession {
    pub id: PlayerId,
    pub name: String,
    pub entity: EntityId,
    pub dimension: ResourceKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServerError {
    #[error("the session is closed")]
    SessionClosed,
    #[error(transparent)]
    Network(#[from] NetworkError),
}

impl ServerError {
    pub fn code(&self) -> u16 {
        match self {
            ServerError::SessionClosed => 1,
            ServerError::Network(_) => 2,
        }
    }
}

pub trait Server {
    fn tick(&mut self) -> Tick;

    fn connect(&mut self, connection: Box<dyn Connection>) -> Result<PlayerSession, ServerError>;

    fn disconnect(&mut self, player: PlayerId);

    fn sessions(&self) -> Vec<PlayerSession>;

    fn entity(&self, id: EntityId) -> Option<&dyn Entity>;

    fn chunk_generator(&self) -> &dyn ChunkGenerator;

    fn chunk_storage(&mut self) -> &mut dyn ChunkStorage;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let a = ServerError::SessionClosed.code();
        let b = ServerError::from(NetworkError::Disconnected).code();
        assert_ne!(a, b);
    }
}
