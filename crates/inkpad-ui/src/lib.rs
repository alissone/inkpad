//! inkpad-ui: a declarative widget toolkit designed for e-ink.
//!
//! An app is a state struct implementing [`App`]: `view` builds an element
//! tree from the state, `update` changes the state in response to messages.
//! The runtime renders, finds what changed on screen, and refreshes only
//! that, with the right e-ink waveform.

pub mod apps;
pub mod assets;
pub mod clock;
pub mod el;
pub mod handoff;
pub mod icons;
pub mod keyboard;
pub mod layout;
pub mod runtime;
pub mod sysui;
pub mod widgets;

pub use el::*;
pub use runtime::{app_installed, run_on, run_with, App, Ctx, Env, Reopen, STATUS_H};
pub use widgets::*;

/// Persistent preferences and text storage.
pub use inkpad_store as store;
pub use inkpad_store::Prefs;

pub use inkpad_gfx::{FamilyId, Fonts, GrayImage, Painter, Rect, TextStyle, BLACK, WHITE};
