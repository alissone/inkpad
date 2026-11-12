//! inkpad-ui: a declarative widget toolkit designed for e-ink.
//!
//! An app is a state struct implementing [`App`]: `view` builds an element
//! tree from the state, `update` changes the state in response to messages.
//! The runtime renders, finds what changed on screen, and refreshes only
//! that, with the right e-ink waveform.

pub mod assets;
pub mod clock;
pub mod el;
pub mod icons;
pub mod keyboard;
pub mod layout;
pub mod runtime;
pub mod widgets;

pub use el::*;
pub use runtime::{run_on, App, Ctx, Env, STATUS_H};
pub use widgets::*;

pub use inkpad_gfx::{FamilyId, Fonts, GrayImage, Painter, Rect, TextStyle, BLACK, WHITE};
