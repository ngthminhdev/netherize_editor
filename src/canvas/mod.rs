//! NetherCanvas — Spatial Canvas (Phase A: navigable read-only canvas).
//!
//! A 2D plane that holds **Code Blocks** (definition / callers / callees of a
//! focal symbol) sourced from LSP, navigated keyboard-first. This module is the
//! UI-free, fully unit-tested core: the block/camera model, deterministic
//! relation placement, and spatial focus navigation. Rendering, input routing,
//! and LSP sourcing live in the render/app layers and consume these types.
//!
//! See `docs/superpowers/specs/2026-06-19-spatial-canvas-phaseA-design.md`.

pub mod layout;
pub mod model;
pub mod navigation;

pub use layout::{PlacedRelation, place_relations};
pub use model::{
    BlockId, BlockOrigin, BlockRelation, BlockSnapshot, CARD_HEADER_LINES, CARD_MAX_LINES, Camera,
    CanvasBlock, CanvasInteraction, CanvasSpan, CanvasState, WorldRect,
};
pub use navigation::Dir;
