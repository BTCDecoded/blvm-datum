//! blvm-datum - DATUM Gateway mining protocol module
//!
//! Library exports for testing and integration
//!
//! Note: This module handles DATUM pool communication only.
//! Miners connect via Stratum V2 (blvm-stratum-v2 module).

pub mod api;
pub mod config;
pub mod module;
pub mod datum_protocol;

pub use config::DatumConfig;
pub mod error;
pub mod handlers;
pub mod messages;
pub mod pool;
pub mod server;
pub mod template;

pub use error::DatumError;
pub use pool::*;
pub use module::DatumModule;
pub use server::*;
pub use template::*;
