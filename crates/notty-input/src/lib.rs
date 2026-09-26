//! notty-input: comandos de interfaz y su mapa de teclas. Sin nada de Windows.

mod command;
mod keyspec;

pub use command::{
    Command, UiCommand, apply_overrides, binding, binding_spec, command_for_key, conflict, default_ui_keymap, set_binding,
};
pub use keyspec::{Modifiers, format_key_spec, key_name, parse_key_spec};
