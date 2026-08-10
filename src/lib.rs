//! Foreseer Desktop library: protocol v2, config, auth, and controller.

pub mod auth;
pub mod config;
pub mod controller;
pub mod protocol;
pub mod session;

pub use config::{AppConfig, ForeseerUrlError, validate_bootstrap_server_url, validate_foreseer_url};
pub use controller::{AppState, Controller, ControllerEvent, RuntimeOps};
pub use protocol::{
    NativeCommandV2, NativeEventV2, PROTOCOL_VERSION, parse_command, serialize_event,
};
