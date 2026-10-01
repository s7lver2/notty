//! notty-sign: genera el par de claves Ed25519 de release, firma archivos con él y
//! comprueba firmas contra la clave pública compilada (`notty_update::PUBKEY`).
//! Solo se compila detrás de la feature `sign` — nunca va en el `notty` que se
//! distribuye, solo lo usa quien construye una release (ver `tools/release.ps1`).

use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use std::path::PathBuf;

fn default_key_path() -> PathBuf {
    let home = std::env::var("USERPROFILE").expect("USERPROFILE no definido");
    PathBuf::from(home).join(".notty-release").join("ed25519.key")
}

fn cmd_keygen(key_path: Option<&str>) {
    let path = key_path.map(PathBuf::from).unwrap_or_else(default_key_path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let sk = SigningKey::generate(&mut OsRng);
    std::fs::write(&path, sk.to_bytes()).expect("no se pudo escribir la clave privada");
    println!("Clave privada escrita en {}", path.display());
    println!("Clave pública (pégala en PUBKEY): {:?}", sk.verifying_key().to_bytes());
}

fn cmd_pubkey(key: Option<&str>) {
    let key_path = key.map(PathBuf::from).unwrap_or_else(default_key_path);
    let key_bytes: [u8; 32] = std::fs::read(&key_path).expect("no se pudo leer la clave privada").try_into().expect("clave con longitud inesperada");
    let sk = SigningKey::from_bytes(&key_bytes);
    println!("Clave pública (pégala en PUBKEY): {:?}", sk.verifying_key().to_bytes());
}

fn cmd_sign(file: &str, key: Option<&str>, out: &str) {
    let key_path = key.map(PathBuf::from).unwrap_or_else(default_key_path);
    let key_bytes: [u8; 32] = std::fs::read(&key_path).expect("no se pudo leer la clave privada").try_into().expect("clave con longitud inesperada");
    let sk = SigningKey::from_bytes(&key_bytes);
    let data = std::fs::read(file).expect("no se pudo leer el archivo a firmar");
    let sig = sk.sign(&data);
    std::fs::write(out, sig.to_bytes()).expect("no se pudo escribir la firma");
    println!("Firma escrita en {out}");
}

/// La firma de `file` (64 bytes crudos en `sig`) contra la clave pública compilada.
fn cmd_verify(file: &str, sig: &str) -> bool {
    let data = std::fs::read(file).expect("no se pudo leer el archivo a comprobar");
    let sig = std::fs::read(sig).expect("no se pudo leer la firma");
    let Ok(sig) = <[u8; 64]>::try_from(sig.as_slice()) else { return false };
    notty_update::verify(&data, &sig, &notty_update::PUBKEY)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--keygen") => {
            let key = args.iter().position(|a| a == "--key").and_then(|i| args.get(i + 1)).map(String::as_str);
            cmd_keygen(key);
        }
        Some("--pubkey") => {
            let key = args.iter().position(|a| a == "--key").and_then(|i| args.get(i + 1)).map(String::as_str);
            cmd_pubkey(key);
        }
        Some("--sign") => {
            let file = args.get(1).expect("uso: notty-sign --sign <file> [--key <path>] --out <file>.sig");
            let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).expect("falta --out");
            let key = args.iter().position(|a| a == "--key").and_then(|i| args.get(i + 1)).map(String::as_str);
            cmd_sign(file, key, out);
        }
        Some("--verify") => {
            let file = args.get(1).expect("uso: notty-sign --verify <file> --sig <file>.sig");
            let sig = args.iter().position(|a| a == "--sig").and_then(|i| args.get(i + 1)).expect("falta --sig");
            if cmd_verify(file, sig) {
                println!("firma válida");
            } else {
                println!("firma NO válida");
                std::process::exit(1);
            }
        }
        _ => eprintln!(
            "uso: notty-sign --keygen [--key <path>] | notty-sign --pubkey [--key <path>] | notty-sign --sign <file> --out <file>.sig [--key <path>] | notty-sign --verify <file> --sig <file>.sig"
        ),
    }
}
