//! The mixer model (docs/ARCHITECTURE.md): sessions grouped into apps, the perceptual volume
//! curve, settings. Pure: no UI, no OS calls, everything tested without Windows.

pub mod activity;
pub mod apps;
pub mod curve;
pub mod memory;
pub mod settings;
