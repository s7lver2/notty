//! notty-input: comandos de interfaz y su mapa de teclas. Sin nada de Windows.

mod command;
mod keyspec;

pub use command::{UiCommand, apply_overrides, default_ui_keymap};
pub use keyspec::{Modifiers, parse_key_spec};
