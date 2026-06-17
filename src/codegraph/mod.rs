//! Code Graph HUD domain logic — pure, UI-free, fully unit-tested.
//!
//! Parses the `codegraph` CLI `--json` output ([`cli_json`]), builds a
//! renderable model with blast-radius risk colors ([`model`]), and provides the
//! `hjkl` navigation state machine ([`navigation`]), column layout ([`layout`]),
//! and edge geometry ([`edges`]) consumed by the renderer.

pub mod cli_json;
pub mod edges;
pub mod layout;
pub mod model;
pub mod navigation;
