//! Micro-benchmarks de las piezas internas de notty (sin ventana).
//!
//!   cargo run --release -p notty-ui --example bench
//!
//! Cada medida es la mediana de varias repeticiones. Los archivos se generan en
//! `%TEMP%\notty-bench` (los mismos que usa `tools/bench.ps1`, si ya existen).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use notty_core::{Document, SearchOptions};
use notty_ui::syntax::{Lang, SyntaxCache};

fn median(mut f: impl FnMut() -> Duration, runs: usize) -> Duration {
    let mut v: Vec<Duration> = (0..runs).map(|_| f()).collect();
    v.sort();
    v[v.len() / 2]
}

fn time(f: impl FnOnce()) -> Duration {
    let t = Instant::now();
    f();
    t.elapsed()
}

fn report(name: &str, d: Duration) {
    println!("{name:<44} {:>9.2} ms", d.as_secs_f64() * 1000.0);
}

fn test_file(name: &str, bytes: usize, line: impl Fn(usize) -> String) -> PathBuf {
    let dir = std::env::temp_dir().join("notty-bench");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    if std::fs::metadata(&path).map(|m| m.len() as usize >= bytes).unwrap_or(false) {
        return path;
    }
    let mut s = String::with_capacity(bytes + 256);
    let mut i = 0;
    while s.len() < bytes {
        s.push_str(&line(i));
        i += 1;
    }
    std::fs::write(&path, s).unwrap();
    path
}

const RUST: &str = "pub fn handle(req: &Request, n: usize) -> Result<Response, Error> {\n    let id = req.id + n; // comentario\n    if id % 2 == 0 { return Ok(Response::new(\"par\", id)); }\n    Err(Error::Odd(id))\n}\n\n";
const MD: &str = "## Sección\n\nTexto con **negrita**, *cursiva*, `código` y un [enlace](https://example.com).\n\n- uno\n- dos\n  - [x] tarea\n\n> cita\n\n```rust\nfn main() { println!(\"hola\"); }\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n";

fn log_line(i: usize) -> String {
    format!("2026-09-27 12:00:{} INFO request id={i} path=/api/items/{} status=200 time={}ms\n", i % 60, i * 7, i % 97)
}

fn main() {
    let rust = test_file("code_1mb.rs", 1 << 20, |_| RUST.to_string());
    let md = test_file("readme_1mb.md", 1 << 20, |_| MD.to_string());
    let log10 = test_file("log_10mb.txt", 10 << 20, log_line);
    let log50 = test_file("log_50mb.txt", 50 << 20, log_line);

    println!("== Abrir (leer + decodificar + rope)");
    for (name, p) in [("rust 1MB", &rust), ("log 10MB", &log10), ("log 50MB", &log50)] {
        report(&format!("open_as_document {name}"), median(|| time(|| drop(notty_ui::open_as_document(p).unwrap())), 7));
    }

    let doc = notty_ui::open_as_document(&rust).unwrap().document;
    println!("== Sintaxis (rust 1MB)");
    // Hasta que haya colores: el parseo va en otro hilo en archivos grandes.
    let ready = |cache: &SyntaxCache, doc: &Document| {
        while cache.spans_for(doc, Some(Lang::Rust), 0..60).iter().all(Vec::is_empty) || cache.is_parsing() {
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    report("primer resaltado: bloqueo del hilo de la UI", median(|| {
        let cache = SyntaxCache::default();
        let d = time(|| drop(cache.spans_for(&doc, Some(Lang::Rust), 0..60)));
        ready(&cache, &doc);
        d
    }, 5));
    report("primer resaltado: hasta tener colores", median(|| {
        let cache = SyntaxCache::default();
        time(|| ready(&cache, &doc))
    }, 5));
    let cache = SyntaxCache::default();
    ready(&cache, &doc);
    report("resaltado ya parseado (60 líneas, un frame)", median(|| time(|| drop(cache.spans_for(&doc, Some(Lang::Rust), 5000..5060))), 21));
    let mut edited = notty_ui::open_as_document(&rust).unwrap().document;
    let cache = SyntaxCache::default();
    ready(&cache, &edited);
    report("tras teclear 1 carácter: bloqueo de la UI", median(|| {
        edited.set_cursor(1000);
        edited.insert("x", Instant::now());
        let d = time(|| drop(cache.spans_for(&edited, Some(Lang::Rust), 0..60)));
        ready(&cache, &edited);
        d
    }, 11));

    println!("== Markdown (1MB)");
    let mdoc = notty_ui::open_as_document(&md).unwrap().document;
    let md_lines = mdoc.buffer().line_strings();
    report("markdown::analyze", median(|| time(|| drop(notty_ui::markdown::analyze(&md_lines))), 5));
    report("extraer líneas del documento", median(|| time(|| drop(mdoc.buffer().line_strings())), 5));

    println!("== Búsqueda (log 50MB, \"status=200\")");
    let big = notty_ui::open_as_document(&log50).unwrap().document;
    report("find_all (buscar de cero)", median(|| time(|| drop(big.find_all("status=200", SearchOptions::default()).unwrap())), 3));
    let mut search = notty_ui::SearchState::default();
    search.set_query("status=200".to_string());
    search.matches(&big).unwrap();
    report("un pintado con el buscador abierto", median(|| time(|| drop(search.matches(&big).unwrap())), 11));

    println!("== Recuperación / copias (log 50MB)");
    report("doc.text() (copia completa)", median(|| time(|| drop(big.text())), 5));
    report("snapshot de recuperación (cada segundo)", median(|| time(|| drop(std::hint::black_box(big.buffer().clone()))), 11));
}
