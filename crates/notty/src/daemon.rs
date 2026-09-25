//! notty --daemon: sin ventana de documento. Sirve el pipe y, más adelante
//! (Task 9), escucha los atajos globales y muestra el icono de bandeja.

pub fn run() -> windows::core::Result<()> {
    // Por ahora, solo sirve el pipe: si alguien reenvía NewTemp/NewPermanent,
    // se limita a lanzar `notty.exe` normal (sin argumentos especiales todavía;
    // la Task 9 completa el reenvío real de "nuevo temporal"/"nuevo permanente").
    let (tx, _rx) = std::sync::mpsc::channel();
    super::spawn_pipe_server(tx);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
