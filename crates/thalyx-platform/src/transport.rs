//! MessageTransport: bytes to a service and bytes back.
//!
//! The managed client never holds a reference into its service. Every request
//! is encoded, handed over, and decoded on the far side, and the reply comes back
//! the same way — so the service can be a thread, a process, or a store inside
//! Thalyx-Kernel reached over a channel, and the client cannot tell which and
//! cannot come to depend on it.
//!
//! [`Loopback`] is the in-process transport. It copies on purpose: a transport
//! that passed a pointer through would let the client and the service share
//! memory by accident, which is exactly the property the other transports do
//! not have.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// The service did not answer: it is gone, or it stopped while the request
    /// was in flight. Whether the request took effect is exactly what nobody can
    /// say from here, and a client must ask rather than assume.
    #[error("the service did not answer: {0}")]
    Gone(String),
}

/// What a transport carried, so a comparison can say how much of a run's cost
/// was moving bytes between a client and its service.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Carried {
    pub calls: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

pub trait Transport {
    fn call(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError>;
    fn carried(&self) -> Carried;
}

/// The far side of a transport.
pub trait Service {
    fn serve(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError>;
}

pub struct Loopback<S: Service> {
    service: S,
    carried: Carried,
}

impl<S: Service> Loopback<S> {
    pub fn new(service: S) -> Self {
        Self {
            service,
            carried: Carried::default(),
        }
    }

    pub fn service(&self) -> &S {
        &self.service
    }

    pub fn service_mut(&mut self) -> &mut S {
        &mut self.service
    }

    pub fn into_service(self) -> S {
        self.service
    }
}

impl<S: Service> Transport for Loopback<S> {
    fn call(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        self.carried.calls += 1;
        self.carried.bytes_sent += request.len() as u64;
        let copied = request.to_vec();
        let reply = self.service.serve(&copied)?;
        self.carried.bytes_received += reply.len() as u64;
        Ok(reply)
    }

    fn carried(&self) -> Carried {
        self.carried
    }
}
