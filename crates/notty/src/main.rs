//! Punto de entrada de notty: interpreta argv y abre la ventana.

fn main() -> windows::core::Result<()> {
    let path = std::env::args().nth(1);
    notty_ui::window::run(path.as_deref())
}
