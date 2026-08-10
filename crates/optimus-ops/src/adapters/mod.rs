//! spec-017 adapters: one module per transport, each owning its config shape
//! and its mock/live transports. Every module exposes the `open_adapter`
//! convention so the supervisor builds the whole registry without knowing
//! transports (see `crate::transport::AdapterBuilder`).

pub mod discord;
pub mod email;
pub mod slack;
