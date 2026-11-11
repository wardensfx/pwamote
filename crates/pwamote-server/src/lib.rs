pub mod actions;
pub mod bootstrap;
pub mod cli;
pub mod clipboard;
pub mod config;
pub mod firewall;
pub mod headless;
pub mod input;
pub mod logging;
pub mod mdns;
pub mod net;
pub mod platform;
pub mod public_cert;
pub mod report;
pub mod session;
pub mod state;
pub mod supervisor;
pub mod tls;
pub mod update;
pub mod web;

/// Le cœur, pour l’application de bureau (types du panneau d’actions).
pub use pwamote_core;
