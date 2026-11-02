use bwr_serializer::{SerializerError, Value};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolState {
    Handshake,
    Status,
    Login,
    Play,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NetworkError {
    #[error("the connection was closed")]
    Disconnected,
    #[error("malformed packet: {0}")]
    Malformed(String),
    #[error(transparent)]
    Serializer(#[from] SerializerError),
}

impl NetworkError {
    pub fn code(&self) -> u16 {
        match self {
            NetworkError::Disconnected => 1,
            NetworkError::Malformed(_) => 2,
            NetworkError::Serializer(_) => 3,
        }
    }
}

pub trait Packet: Into<Value> + TryFrom<Value, Error = SerializerError> {
    const ID: u32;
    const STATE: ProtocolState;
}

pub trait PacketCodec {
    fn encode(&self, id: u32, value: &Value) -> Result<Vec<u8>, NetworkError>;

    fn decode(&self, bytes: &[u8]) -> Result<(u32, Value), NetworkError>;
}

pub trait Connection {
    fn state(&self) -> ProtocolState;

    fn send(&mut self, id: u32, value: Value) -> Result<(), NetworkError>;

    fn receive(&mut self) -> Result<Option<(u32, Value)>, NetworkError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let codes = [
            NetworkError::Disconnected.code(),
            NetworkError::Malformed(String::new()).code(),
            NetworkError::from(SerializerError::Format(String::new())).code(),
        ];
        assert_eq!(codes, [1, 2, 3]);
    }
}
