//! Teclado ISO español dibujado con Direct2D: el del recorrido guiado (`tour.rs`) y el
//! de la captura de atajos de Ajustes (`settings_window.rs`). Solo pinta la rejilla de
//! teclas; el panel de fondo y los textos alrededor son cosa de quien lo llama, igual
//! que la opacidad (`Renderer::set_fade`).

use crate::Renderer;
use crate::layout::Rect;
use crate::theme::{Palette, Rgba};

pub const GREEN: Rgba = Rgba(0.30, 0.75, 0.45, 1.0);

/// Ancho y alto de la rejilla en unidades de tecla (cada fila suma 15).
pub const UNITS_W: f32 = 15.0;
pub const UNITS_H: f32 = 5.0;
const GAP: f32 = 3.0;

/// Teclado ISO español simplificado: `(etiqueta, ancho en unidades, VK)`. `0` = tecla
/// que no se puede detectar ni resaltar.
const KEY_ROWS: &[&[(&str, f32, u32)]] = &[
    &[
        ("º", 1.0, 0xDC), ("1", 1.0, 0x31), ("2", 1.0, 0x32), ("3", 1.0, 0x33), ("4", 1.0, 0x34), ("5", 1.0, 0x35),
        ("6", 1.0, 0x36), ("7", 1.0, 0x37), ("8", 1.0, 0x38), ("9", 1.0, 0x39), ("0", 1.0, 0x30), ("'", 1.0, 0xDB),
        ("¡", 1.0, 0xDD), ("⌫", 2.0, 0x08),
    ],
    &[
        ("Tab", 1.5, 0x09), ("Q", 1.0, 0x51), ("W", 1.0, 0x57), ("E", 1.0, 0x45), ("R", 1.0, 0x52), ("T", 1.0, 0x54),
        ("Y", 1.0, 0x59), ("U", 1.0, 0x55), ("I", 1.0, 0x49), ("O", 1.0, 0x4F), ("P", 1.0, 0x50), ("`", 1.0, 0xBA),
        ("+", 1.0, 0xBB), ("↵", 1.5, 0x0D),
    ],
    &[
        ("Bloq", 1.75, 0x14), ("A", 1.0, 0x41), ("S", 1.0, 0x53), ("D", 1.0, 0x44), ("F", 1.0, 0x46), ("G", 1.0, 0x47),
        ("H", 1.0, 0x48), ("J", 1.0, 0x4A), ("K", 1.0, 0x4B), ("L", 1.0, 0x4C), ("Ñ", 1.0, 0xC0), ("´", 1.0, 0xDE),
        ("Ç", 1.0, 0xBF), ("", 1.25, 0),
    ],
    &[
        ("Shift", 1.25, 0x10), ("<", 1.0, 0xE2), ("Z", 1.0, 0x5A), ("X", 1.0, 0x58), ("C", 1.0, 0x43), ("V", 1.0, 0x56),
        ("B", 1.0, 0x42), ("N", 1.0, 0x4E), ("M", 1.0, 0x4D), (",", 1.0, 0xBC), (".", 1.0, 0xBE), ("-", 1.0, 0xBD),
        ("Shift", 2.75, 0x10),
    ],
    &[
        ("Ctrl", 1.25, 0x11), ("Win", 1.25, 0x5B), ("Alt", 1.25, 0x12), ("", 6.25, 0x20), ("AltGr", 1.25, 0xA5),
        ("Win", 1.25, 0x5C), ("Menú", 1.25, 0x5D), ("Ctrl", 1.25, 0x11),
    ],
];

pub fn key_down(vk: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    vk != 0 && unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0
}

/// Teclas del dibujo que están físicamente pulsadas ahora mismo.
pub fn held_keys() -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for row in KEY_ROWS {
        for &(_, _, vk) in row.iter() {
            if vk != 0 && !out.contains(&vk) && key_down(vk) {
                out.push(vk);
            }
        }
    }
    out
}

/// VKs que hay que resaltar para un atajo ya resuelto (tecla + modificadores).
pub fn combo_vks(vk: u32, m: notty_input::Modifiers) -> Vec<u32> {
    let mut out = Vec::new();
    if m.ctrl {
        out.push(0x11);
    }
    if m.shift {
        out.push(0x10);
    }
    if m.alt {
        out.push(0x12);
    }
    out.push(vk);
    out
}

/// Tamaño de tecla que cabe en `avail_w` (mismos límites que el recorrido).
pub fn unit_for_width(avail_w: f32) -> f32 {
    (avail_w / UNITS_W).clamp(14.0, 26.0)
}

/// Pinta la rejilla con la esquina superior izquierda en `(left, top)` y teclas de `u`
/// DIPs. `highlighted`: teclas del atajo, en azul respirando según `pulse` (`0..=1`),
/// virando a verde con `confirm` (`0..=1`). `pressed`: en verde. Devuelve el rect.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    r: &Renderer,
    pal: &Palette,
    left: f32,
    top: f32,
    u: f32,
    highlighted: &[u32],
    pressed: &[u32],
    pulse: f32,
    confirm: f32,
) -> Rect {
    let dark_green_fg = Rgba(0.04, 0.10, 0.06, 1.0);
    let dark_blue_fg = Rgba(0.04, 0.07, 0.12, 1.0);
    let mut y = top;
    for row in KEY_ROWS {
        let mut x = left;
        for &(label, wu, vk) in row.iter() {
            let kr = Rect::new(x, y, x + wu * u - GAP, y + u - GAP);
            let wanted = vk != 0 && highlighted.contains(&vk);
            let held = vk != 0 && pressed.contains(&vk);
            let (fill, fg) = if held {
                (GREEN, dark_green_fg)
            } else if wanted {
                let a = 0.70 + 0.30 * pulse;
                let blue = Rgba(pal.accent.0, pal.accent.1, pal.accent.2, a);
                if confirm > 0.0 { (blue.mix(GREEN, confirm), dark_blue_fg.mix(dark_green_fg, confirm)) } else { (blue, dark_blue_fg) }
            } else {
                (pal.hover, pal.text_3)
            };
            r.fill_round(kr, 4.0, fill);
            if !label.is_empty() {
                r.text_center(label, &r.fonts().ui_10_5, kr, fg);
            }
            x += wu * u;
        }
        y += u;
    }
    Rect::new(left, top, left + u * UNITS_W, top + u * UNITS_H)
}
