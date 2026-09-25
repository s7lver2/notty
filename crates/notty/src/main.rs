//! Punto de entrada de notty: interpreta argv, carga la configuración y abre la ventana.

fn main() -> windows::core::Result<()> {
    let path = std::env::args().nth(1);
    let cfg = match notty_config::load(&notty_config::default_path()) {
        notty_config::LoadResult::Loaded(cfg) | notty_config::LoadResult::Missing(cfg) => cfg,
        notty_config::LoadResult::Defaulted(cfg, _msg) => cfg, // el aviso se muestra en pantalla en la Task 9
    };
    notty_ui::window::run(path.as_deref(), cfg)
}
