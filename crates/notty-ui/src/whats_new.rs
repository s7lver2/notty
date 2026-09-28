//! "Novedades": el popup que sale la primera vez que se abre notty tras actualizarse,
//! con lo nuevo de esa versión (y que se puede volver a ver en Ayuda → Novedades).
//!
//! Para cada versión nueva basta con añadir su entrada al principio de `RELEASES`: una
//! lista corta de novedades, cada una con una miniatura animada de `Icon`.

/// Qué miniatura animada lleva cada novedad (se dibujan en `render.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Un markdown que se va renderizando: las marcas se convierten en texto con formato.
    Markdown,
    /// Líneas de velocidad.
    Speed,
    /// Un punto recorriendo una onda.
    Animations,
    /// Destello genérico.
    Sparkle,
}

#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub icon: Icon,
    pub title: &'static str,
    pub desc: &'static str,
}

pub struct Release {
    pub version: &'static str,
    pub items: &'static [Item],
}

/// La más nueva primero.
pub const RELEASES: &[Release] = &[Release {
    version: "0.6.0",
    items: &[
        Item {
            icon: Icon::Markdown,
            title: "Previsualización de Markdown",
            desc: "Los .md se ven como en GitHub: código con colores, tablas, citas y tareas. Ctrl+Shift+M.",
        },
        Item {
            icon: Icon::Speed,
            title: "Hasta 8 veces más rápida al abrir",
            desc: "Los archivos grandes se abren al momento y notty ya no gasta CPU en reposo.",
        },
        Item {
            icon: Icon::Animations,
            title: "Animaciones a tu medida",
            desc: "Elige 30, 60 o 120 Hz, o animaciones reducidas, en Ajustes → Ventana.",
        },
    ],
}];

/// Las novedades de `version`, o las de la última versión con novedades si esa no tiene
/// (para Ayuda → Novedades en una versión sin entrada propia).
pub fn release_for(version: &str) -> Option<&'static Release> {
    RELEASES.iter().find(|r| r.version == version).or(RELEASES.first())
}

/// Si hay que enseñar el popup al arrancar. `last_seen` es la versión con la que se
/// abrió notty la última vez (vacía si nunca se apuntó); en una instalación nueva
/// (`first_run_done` todavía `false`) sale la bienvenida, no esto.
pub fn should_show(last_seen: &str, first_run_done: bool, current: &str) -> bool {
    first_run_done && last_seen != current && RELEASES.iter().any(|r| r.version == current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_once_after_updating_to_a_version_with_news() {
        let v = RELEASES[0].version;
        assert!(should_show("0.5.0", true, v));
        // Actualizado desde una versión que aún no apuntaba la última vista.
        assert!(should_show("", true, v));
        assert!(!should_show(v, true, v), "ya se vio");
    }

    #[test]
    fn not_on_a_fresh_install_nor_without_news() {
        assert!(!should_show("", false, RELEASES[0].version));
        assert!(!should_show("0.5.0", true, "99.0.0"));
    }

    #[test]
    fn release_for_falls_back_to_the_newest() {
        assert_eq!(release_for("99.0.0").map(|r| r.version), Some(RELEASES[0].version));
        assert!(RELEASES.iter().all(|r| !r.items.is_empty()));
    }
}
