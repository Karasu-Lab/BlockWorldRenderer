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

#[cfg(test)]
mod tests {
    use super::*;

    struct Feature {
        client: Option<u8>,
        server: Option<u8>,
    }

    impl Sided for Feature {
        type Client = u8;
        type Server = u8;

        fn client(&self) -> Option<&u8> {
            self.client.as_ref()
        }

        fn server(&self) -> Option<&u8> {
            self.server.as_ref()
        }
    }

    #[test]
    fn server_only_is_taken_from_the_server() {
        let feature = Feature {
            client: None,
            server: Some(2),
        };
        assert!(matches!(feature.dispatch(), Ok(Dispatch::UseServer(2))));
    }

    #[test]
    fn client_only_is_requested_from_the_client() {
        let feature = Feature {
            client: Some(1),
            server: None,
        };
        assert!(matches!(feature.dispatch(), Ok(Dispatch::RequestClient(1))));
    }

    #[test]
    fn both_sides_run_their_own() {
        let feature = Feature {
            client: Some(1),
            server: Some(2),
        };
        assert!(matches!(
            feature.dispatch(),
            Ok(Dispatch::Both {
                client: 1,
                server: 2
            })
        ));
    }

    #[test]
    fn neither_side_is_an_error() {
        let feature = Feature {
            client: None,
            server: None,
        };
        assert_eq!(feature.dispatch().err(), Some(SideError::Unavailable));
    }
}
