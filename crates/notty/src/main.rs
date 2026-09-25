//! Punto de entrada de notty: interpreta argv, carga la configuración y abre la ventana.

fn main() -> windows::core::Result<()> {
    let path = std::env::args().nth(1);
    let load = notty_config::load(&notty_config::default_path());
    notty_ui::window::run(path.as_deref(), load)
}
