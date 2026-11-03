use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Client,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SideError {
    #[error("neither the client nor the server provides this")]
    Unavailable,
}

impl SideError {
    pub fn code(&self) -> u16 {
        match self {
            SideError::Unavailable => 1,
        }
    }
}

pub enum Dispatch<'a, C: ?Sized, S: ?Sized> {
    Both { client: &'a C, server: &'a S },
    UseServer(&'a S),
    RequestClient(&'a C),
}

pub trait Sided {
    type Client: ?Sized;
    type Server: ?Sized;

    fn client(&self) -> Option<&Self::Client>;

    fn server(&self) -> Option<&Self::Server>;

    fn dispatch(&self) -> Result<Dispatch<'_, Self::Client, Self::Server>, SideError> {
        match (self.client(), self.server()) {
            (Some(client), Some(server)) => Ok(Dispatch::Both { client, server }),
            (None, Some(server)) => Ok(Dispatch::UseServer(server)),
            (Some(client), None) => Ok(Dispatch::RequestClient(client)),
            (None, None) => Err(SideError::Unavailable),
        }
    }
}
