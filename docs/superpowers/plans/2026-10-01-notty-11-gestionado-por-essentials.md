# notty 1.1 · Gestionado por essentials, tema compartido y MSI firmado Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que notty 1.1.0 se deje gestionar por la tienda essentials: `release.ps1` deja (y publica) `notty.msi` + `notty.msi.sig` firmados y comprobados, el MSI registra su carpeta (`ARPINSTALLLOCATION`), `[updates] managed_by = "essentials"` apaga el actualizador propio y cambia Ajustes → Actualizaciones por la tarjeta «Gestionado por essentials», y notty sigue en caliente el tema y el acento de `%APPDATA%\essentials\appearance.toml`.

**Architecture:** `notty-config` gana el campo `UpdatesConfig::managed_by` (y `save` conserva el que haya en disco), `UiConfig::follow_essentials` y un módulo puro `appearance` (leer/escribir `appearance.toml` con `toml`). `notty-ui` gana `shared_theme.rs`: estado global (existe el archivo / sus valores), `effective(&UiConfig)` que sustituye tema y acento donde hoy se leen `cfg.ui.theme`/`cfg.ui.accent`, la escritura al cambiar tema o acento siguiendo a essentials, y un hilo con `ReadDirectoryChangesW` que avisa a la ventana principal (`WM_SHARED_THEME`); el fundido de 350 ms es el que ya existe (`last_dark`/`last_accent` en `window.rs`, `theme_from`/`accent_from` en `settings_window.rs`). Para probar sin tocar el notty real, `NOTTY_PRUEBAS=1` desactiva registro, pipe de instancia única y atajo global, y `tools/shot-settings.ps1` lanza notty en un perfil de pruebas y captura por HWND.

**Tech Stack:** Rust 2024, `windows` 0.62.2 (`Win32_UI_Shell` `ShellExecuteExW`, `Win32_Storage_FileSystem` + `Win32_System_IO` + `Win32_Security` para `CreateFileW`/`ReadDirectoryChangesW`), `toml` 0.8, `ed25519-dalek` 2 (ya están), WiX 5 (`wix build`), PowerShell 7.

**Spec:** `E:\essentials\docs\superpowers\specs\2026-10-01-essentials-migracion-design.md` §«En notty y transppy» puntos 1, 2, 3, 4 y 6 (el 5 es solo de transppy), §«Compatibilidad» y §«Verificación»; con las decisiones comunes del coordinador (contrato) copiadas abajo donde aplican.

**Depende de:** nada de otros planes. Comprueba la rama y el punto de partida en la Task 1, Step 1.

## Rama

Se trabaja en `essentials-gestionado`, que ya existe y se creó desde `traduce-actualizacion`, no desde `master`: `traduce-actualizacion` tiene un commit publicado en origin, `9aeebdc` («fix: traduce del todo el aviso de versión nueva y Ajustes → Actualizaciones»), que todavía no está en `master` y que toca las mismas páginas que este plan (Ajustes → Actualizaciones, `strings.rs`, `CHANGELOG.md` «Sin publicar»). Partir de `master` obligaría a rehacerlo o a resolver conflictos después. No se crea ninguna rama nueva, no se hace merge ni push.

## Global Constraints

**Seguridad (obligatorio en cada paso):**
- cwd siempre `E:\notty`; en los commits, `git -C E:\notty …`. `git add` solo con rutas explícitas.
- Caché y temporales en E: (notty no tiene `env.ps1`). Antes de **cada** `cargo`, `wix` o `tools\release.ps1`, en la misma sesión de PowerShell:
  ```powershell
  New-Item -ItemType Directory -Force E:\cache\tmp | Out-Null
  $env:CARGO_TARGET_DIR="E:\notty\target"; $env:TEMP="E:\cache\tmp"; $env:TMP="E:\cache\tmp"
  ```
  No cambies `CARGO_HOME`.
- RAM libre ≥ 4 GB antes de cada build: `[math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 1)` (GB). Si sale < 4, espera y vuelve a mirar.
- Los builds de release son largos (LTO): lanza `cargo build --release` y `tools\release.ps1` con un tiempo máximo de 10 min (o en segundo plano y espera a que acabe).
- **Perfil de pruebas.** notty se lanza **solo** con `tools\shot-settings.ps1` (Task 2), que pone `APPDATA=E:\notty\.cache\appdata\Roaming`, `LOCALAPPDATA=E:\notty\.cache\appdata\Local` y `NOTTY_PRUEBAS=1` solo para el proceso hijo y las restaura enseguida. El `appearance.toml` de prueba va en `E:\notty\.cache\appdata\Roaming\essentials\appearance.toml`. **No lances ningún notty antes de terminar la Task 2** (sin `NOTTY_PRUEBAS` en el binario, un notty de pruebas escribe `HKCU\…\Run`, cierra el daemon real del usuario y comparte el pipe de instancia única con su notty real).
- **Instancia única.** Investigado: la tubería es fija (`\\.\pipe\notty-instance`, `crates/notty-ipc/src/lib.rs:1`) y no depende de `APPDATA`; un notty solo reenvía si recibe una ruta y `open_in_existing_window` (`crates/notty/src/main.rs:152-159`), pero toda instancia con ventana **sirve** la tubería (`main.rs:169`) y podría quedarse los archivos que el usuario abre en su notty real. Con `NOTTY_PRUEBAS=1` (Task 2) el notty de pruebas ni reenvía ni sirve la tubería, así que puede convivir con el notty real abierto (ahora mismo hay uno, `C:\Program Files\notty\notty.exe`): **no lo cierres ni lo toques**. `tools\shot-settings.ps1` se niega a arrancar si ya hay un `E:\notty\target\release\notty.exe` abierto que no lanzó él.
- Procesos: `Start-Process -PassThru` y matar **solo por PID** (`Stop-Process -Id`). Nunca por nombre.
- Borrar: solo `Remove-Item -LiteralPath "<ruta completa>"`, nunca comodines ni variables posiblemente vacías.
- **Nunca** instalar, desinstalar ni actualizar notty: prohibido `msiexec`, abrir/ejecutar `notty.msi` o `notty-setup.exe`, `Start-Process *.msi`, doble clic. Nunca `Get-CimInstance Win32_Product`. El `.msi` solo se abre en **solo lectura** con el COM `WindowsInstaller.Installer` (`OpenDatabase(ruta, 0)`).
- No tocar `%APPDATA%\notty`, `%APPDATA%\essentials`, `%LOCALAPPDATA%\notty` reales (ni leerlos). **No escribir en el registro** (leer `HKCU:\…\Run` y `App Paths` para comprobar está permitido).
- No publicar: `tools\release.ps1` **siempre** con `-NoPublish`; nada de `gh`, nada de `git push`.
- Claves de firma: `C:\Users\NICKE\.notty-release\ed25519.key` existe (comprobado). Se usa sin imprimirla, copiarla ni regenerarla; `notty-sign --keygen` y `--pubkey` **prohibidos**. Si falta: PARA y avisa.
- Capturas: solo ventanas de notty de pruebas, por su HWND/PID (`tools\shot-settings.ps1`). Revísalas con Read.
- No modificar otros repos (`E:\essentials`, `E:\transppy` solo lectura).

**Proyecto:**
- Sin tests nuevos. Los existentes de los crates tocados deben pasar: `cargo test -p notty-config -p notty-update -p notty-ui -p notty` (si alguno rompe por el cambio, arréglalo).
- Cada tarea: `cargo build --release -p notty` (o `release.ps1` donde se diga) + prueba manual concreta; en UI, captura por HWND revisada con Read y comparada con el lenguaje de notty: tokens de `crates/notty-ui/src/theme.rs` (`surface_2`, `text`, `text_2`, `danger`, `accent`, `accent_soft`), los controles de `crates/notty-ui/src/settings_ui.rs` (`row_c` = `surface_2`, `button` radio 6 y texto 12) y la tarjeta de estado que ya existe en Ajustes → Actualizaciones (`settings_pages.rs:1813-1858`: radio 10, icono en círculo de 28, título 16 seminegrita, descripción 12 `text_2`, botón a la derecha), además de `docs/mockups/ref/ajustes.png` para el resto de Ajustes.
- Un commit por tarea con el trailer `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Código lo más simple posible (ponytail). Textos en español de España; cada texto nuevo de la interfaz lleva su par inglés en `crates/notty-ui/src/strings.rs` (`TABLE`, sin claves repetidas: hay un test).
- Si una firma de `windows` 0.62 difiere de la escrita, sigue al compilador sin cambiar el comportamiento.
- Variable de prueba nueva: `NOTTY_PRUEBAS` (si existe: sin `notepad::sync`, sin pipe de instancia única ni reenvío, y `global_hotkey::sync` no hace nada).

## Archivos

| Archivo | Qué |
|---|---|
| `tools/release.ps1` | `-NoPublish`, `notty.msi` a `target\release\`, firma y comprobación de `.exe` y `.msi`, sube los cuatro con `gh`. |
| `installer/notty.wxs` | `SetProperty ARPINSTALLLOCATION = [INSTALLFOLDER]` tras `CostFinalize`. |
| `crates/notty-update/src/bin/notty_sign.rs` | `--verify <archivo> --sig <archivo.sig>` contra `PUBKEY`. |
| `.gitignore` | `/.cache/` (perfil de pruebas y capturas). |
| `crates/notty/src/main.rs` | `NOTTY_PRUEBAS`; sin comprobación al arrancar si `managed_by`. |
| `crates/notty-ui/src/global_hotkey.rs` | `sync` no hace nada con `NOTTY_PRUEBAS`. |
| `tools/shot-settings.ps1` | Lanza notty en el perfil de pruebas, abre Ajustes, clics/teclas por mensaje y captura por HWND. |
| `crates/notty-config/src/{model,storage,lib}.rs` | `managed_by`, `managed()`, `follow_essentials`, `save` que conserva `managed_by`, `save_as_is`. |
| `crates/notty-config/src/appearance.rs` | Leer y escribir `appearance.toml`. |
| `crates/notty-ui/src/shared_theme.rs`, `lib.rs` | Estado del tema compartido, `effective`, `push`, vigilancia. |
| `crates/notty-ui/src/window.rs` | Sin comprobar/descargar si gestionado, relee `managed_by` al abrir Ajustes, tema efectivo, `WM_SHARED_THEME`. |
| `crates/notty-ui/src/settings_{window,pages,model}.rs` | Tarjeta «Gestionado por essentials», «Abrir en essentials», fila «Seguir el tema de essentials», fundido al cambiar desde fuera. |
| `crates/notty-ui/src/strings.rs` | Traducciones de los textos nuevos. |
| `crates/notty-ui/Cargo.toml` | Features de `windows` para la vigilancia. |
| `crates/notty/Cargo.toml`, `crates/notty-setup/Cargo.toml`, `Cargo.lock` | Versión 1.1.0. |
| `CHANGELOG.md`, `crates/notty-ui/src/whats_new.rs` | Entrada 1.1.0 y Novedades. |

## Desviaciones del spec (decididas, documentadas)

- La tabla de actualizaciones de notty es `[updates]`, no `[update]`: la clave es `[updates] managed_by = "essentials"` (essentials escribe en `updates` para el id `notty`).
- notty no tiene `[appearance]` (su tema vive en `[ui]`): el ajuste es `[ui] follow_essentials = true` (por defecto).
- notty no tiene «Ajustes → General»: la fila «Seguir el tema de essentials» va en Ajustes → Apariencia, que es donde están Tema y Color de acento.
- La tarjeta «Gestionado por essentials» reutiliza la tarjeta de estado que ya tiene la página Actualizaciones (radio 10, título 16) en vez de inventar una con radio 8 y título 14: es el patrón real de notty en esa misma página.
- `ShellExecuteExW` (no `ShellExecuteW`) con `SEE_MASK_FLAG_NO_UI`, para saber si falló sin que Windows saque su propio diálogo.
- Añadido para poder probar sin romper el notty real: `NOTTY_PRUEBAS` y `tools/shot-settings.ps1` (Task 2).

---

### Task 1: MSI firmado, `--verify` y `ARPINSTALLLOCATION`

**Files:**
- Modify: `tools/release.ps1` (entero)
- Modify: `installer/notty.wxs:8` (añadir una línea detrás)
- Modify: `crates/notty-update/src/bin/notty_sign.rs` (entero)
- Modify: `.gitignore` (añadir una línea)

**Interfaces:**
- Produces:
  ```text
  notty-sign --verify <archivo> --sig <archivo.sig>   → "firma válida" (código 0) | "firma NO válida" (código 1)
  tools/release.ps1 -Tag vX.Y.Z [-NotesFile f] [-NoPublish]
      → target\release\notty-setup.exe(.sig), target\release\notty.msi(.sig), las dos firmas comprobadas
  ```
- Versión: esta tarea se verifica con la versión **actual** (`-Tag v1.0.0`); la subida a 1.1.0 es la Task 6 y el `release.ps1 -Tag v1.1.0 -NoPublish` final va en la Task 7.

- [ ] **Step 1: Rama y punto de partida**

Run:
```powershell
git -C E:\notty branch --show-current
git -C E:\notty merge-base --is-ancestor 9aeebdc HEAD; "ancestro: $LASTEXITCODE"
git -C E:\notty status --short
Test-Path C:\Users\NICKE\.notty-release\ed25519.key
```
Expected: `essentials-gestionado`, `ancestro: 0`, estado limpio (como mucho este plan sin commitear en `docs/superpowers/plans/`), `True`. Si la rama no es esa, si sale otro código o si la clave no existe: **PARA** y avisa.

- [ ] **Step 2: `--verify` en `notty-sign`**

`E:\notty\crates\notty-update\src\bin\notty_sign.rs` (contenido completo):
```rust
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
```

- [ ] **Step 3: `ARPINSTALLLOCATION` en el `.wxs`**

En `E:\notty\installer\notty.wxs`, justo debajo de la línea 8 (`    <Property Id="INSTALLFOLDER" Secure="yes" />`), añade (igual que `E:\essentials\test\msi\prueba.wxs:21`):
```xml
    <!-- Para que MsiGetProductInfoW(INSTALLLOCATION) devuelva la carpeta (la usa essentials). -->
    <SetProperty Id="ARPINSTALLLOCATION" Value="[INSTALLFOLDER]" After="CostFinalize" />
```

- [ ] **Step 4: `release.ps1` con `-NoPublish`, MSI firmado y comprobado**

Hoy (`tools/release.ps1`, 65 líneas): no tiene `-NoPublish` (`:5-8`), exige árbol limpio siempre (`:23-24`), no fija el directorio (usa rutas relativas al cwd), deja el MSI en `installer\notty.msi` (`:42-47`, que `crates/notty-setup/build.rs:9-11` embebe), firma solo el `.exe` con `cargo run` (`:56-58`) y sube solo `.exe`+`.sig` (`:62`). `E:\notty\tools\release.ps1` (contenido completo):
```powershell
# Construye, firma y publica una release de notty: notty.msi (instalador WiX) y
# notty-setup.exe (con el MSI dentro), los dos firmados con notty-sign (Ed25519, clave en
# %USERPROFILE%\.notty-release\ed25519.key) y comprobados contra la clave pública
# compilada (PUBKEY), subidos a GitHub Releases con `gh`.
#
#   pwsh tools/release.ps1 -Tag v1.3.0 -NotesFile notas.md
#   pwsh tools/release.ps1 -Tag v1.3.0 -NoPublish      # todo menos `gh release create`
#
# Deja en target\release\: notty-setup.exe(.sig) y notty.msi(.sig). No instala nada.
# Antes de llamarlo, CARGO_TARGET_DIR=<repo>\target (las rutas de abajo cuentan con ello).
param(
    [Parameter(Mandatory=$true)][string]$Tag,   # e.g. v1.3.0
    [string]$NotesFile,
    [switch]$NoPublish
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

$Repo = "s7lver2/notty"

# Compara un tag ("v1.3.0") con la versión de Cargo.toml ("1.3.0"), sin el
# prefijo "v".
function Test-TagMatchesVersion {
    param([string]$Tag, [string]$CargoVersion)
    return ($Tag -replace '^v', '') -eq $CargoVersion
}

# 1. Árbol de trabajo limpio (solo si se publica) + el tag coincide con crates/notty/Cargo.toml.
if (-not $NoPublish) {
    $status = git status --porcelain
    if ($status) { throw "El árbol de trabajo no está limpio. Confirma o descarta los cambios primero." }
}

$version = ($Tag -replace '^v', '')
$cargoVersion = (Select-String -Path "crates\notty\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if (-not (Test-TagMatchesVersion -Tag $Tag -CargoVersion $cargoVersion)) {
    throw "El tag $Tag no coincide con la versión de crates/notty/Cargo.toml ($cargoVersion)."
}
# notty-setup compara su propia versión con la última release para autoactualizarse:
# si se quedara atrás, cada instalador recién descargado se volvería a descargar.
$setupVersion = (Select-String -Path "crates\notty-setup\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if (-not (Test-TagMatchesVersion -Tag $Tag -CargoVersion $setupVersion)) {
    throw "El tag $Tag no coincide con la versión de crates/notty-setup/Cargo.toml ($setupVersion)."
}

# 2. Build: notty/notty-legacy, el MSI (WiX) y notty-setup (que embebe installer\notty.msi).
cargo build --release -p notty -p notty-legacy
if ($LASTEXITCODE -ne 0) { throw "Fallo compilando notty/notty-legacy" }

wix build installer\notty.wxs -arch x64 `
    -d ProductVersion=$version `
    -d NottyExePath=target\release\notty.exe `
    -d NottyLegacyExePath=target\release\notepad_legacy.exe `
    -o installer\notty.msi
if ($LASTEXITCODE -ne 0) { throw "Fallo construyendo el MSI" }

# El MSI suelto también es un asset (lo instala essentials).
$msi = "target\release\notty.msi"
Copy-Item installer\notty.msi $msi -Force

cargo build --release -p notty-setup
if ($LASTEXITCODE -ne 0) { throw "Fallo compilando notty-setup" }

$setupExe = "target\release\notty-setup.exe"

# 3. Firmas (notty-setup.exe y notty.msi) y su comprobación con la clave pública compilada.
cargo build --release -p notty-update --features sign --bin notty-sign
if ($LASTEXITCODE -ne 0) { throw "Fallo compilando notty-sign" }
$sign = "target\release\notty-sign.exe"
foreach ($f in @($setupExe, $msi)) {
    & $sign --sign $f --out "$f.sig"
    if ($LASTEXITCODE -ne 0) { throw "Fallo firmando $f" }
    & $sign --verify $f --sig "$f.sig"
    if ($LASTEXITCODE -ne 0) { throw "La firma de $f no se verifica con PUBKEY" }
}

# 4. Publica en GitHub Releases.
if ($NoPublish) {
    Write-Host "Listo sin publicar: $setupExe, $msi y sus .sig" -ForegroundColor Yellow
    return
}
if (-not $NotesFile) { throw "Pasa -NotesFile con las notas de la versión." }
gh release create $Tag $setupExe "$setupExe.sig" $msi "$msi.sig" --repo $Repo --notes-file $NotesFile
if ($LASTEXITCODE -ne 0) { throw "Fallo publicando la release en GitHub" }

Write-Host "Release $Tag publicada." -ForegroundColor Green
```

- [ ] **Step 5: `.cache/` ignorado**

Añade al final de `E:\notty\.gitignore` la línea:
```text
/.cache/
```

- [ ] **Step 6: Generar sin publicar y comprobar firmas**

Run (con las variables de entorno de Global Constraints puestas y RAM ≥ 4 GB):
```powershell
Set-Location E:\notty
pwsh -NoProfile -File tools\release.ps1 -Tag v1.0.0 -NoPublish
Get-Item E:\notty\target\release\notty.msi, E:\notty\target\release\notty.msi.sig, E:\notty\target\release\notty-setup.exe.sig | Select-Object Name, Length
$sign = "E:\notty\target\release\notty-sign.exe"
& $sign --verify E:\notty\target\release\notty.msi --sig E:\notty\target\release\notty.msi.sig; "código $LASTEXITCODE"
New-Item -ItemType Directory -Force E:\notty\.cache\t1 | Out-Null
Copy-Item -LiteralPath E:\notty\target\release\notty.msi -Destination E:\notty\.cache\t1\alterado.msi -Force
[System.IO.File]::AppendAllText("E:\notty\.cache\t1\alterado.msi", "x")
& $sign --verify E:\notty\.cache\t1\alterado.msi --sig E:\notty\target\release\notty.msi.sig; "código $LASTEXITCODE"
Remove-Item -LiteralPath "E:\notty\.cache\t1\alterado.msi"
git -C E:\notty status --short
```
Expected: el script imprime dos veces `Firma escrita en …` y dos `firma válida`, y acaba con `Listo sin publicar: target\release\notty-setup.exe, target\release\notty.msi y sus .sig`; `notty.msi.sig` y `notty-setup.exe.sig` miden `64`; la primera comprobación `firma válida` / `código 0`; la del alterado `firma NO válida` / `código 1`. `git status` enseña solo `.gitignore`, `installer/notty.wxs`, `tools/release.ps1`, `crates/notty-update/src/bin/notty_sign.rs` (y el plan si no está commiteado): `installer/notty.msi` está ignorado y `.cache/` ya no sale.

- [ ] **Step 7: `ARPINSTALLLOCATION` en la tabla del MSI (solo lectura, sin instalar)**

Run en pwsh (no se instala nada: `OpenDatabase(…, 0)` = solo lectura):
```powershell
$msi = "E:\notty\target\release\notty.msi"
$inst = New-Object -ComObject WindowsInstaller.Installer
$db = $inst.GetType().InvokeMember("OpenDatabase", "InvokeMethod", $null, $inst, @($msi, 0))
function Get-MsiRows($db, [string]$sql) {
    $view = $db.GetType().InvokeMember("OpenView", "InvokeMethod", $null, $db, @($sql))
    [void]$view.GetType().InvokeMember("Execute", "InvokeMethod", $null, $view, $null)
    $rows = @()
    while ($true) {
        $rec = $view.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $view, $null)
        if ($null -eq $rec) { break }
        $n = $rec.GetType().InvokeMember("FieldCount", "GetProperty", $null, $rec, $null)
        $cells = foreach ($i in 1..$n) { $rec.GetType().InvokeMember("StringData", "GetProperty", $null, $rec, @($i)) }
        $rows += ($cells -join " | ")
    }
    [void]$view.GetType().InvokeMember("Close", "InvokeMethod", $null, $view, $null)
    $rows
}
$ca = @(Get-MsiRows $db "SELECT Action, Type, Source, Target FROM CustomAction WHERE Source='ARPINSTALLLOCATION'")
"CustomAction: $($ca -join ' || ')"
$action = ($ca[0] -split ' \| ')[0]
"Secuencia: $(Get-MsiRows $db "SELECT Action, Sequence FROM InstallExecuteSequence WHERE Action='$action'")"
"CostFinalize: $(Get-MsiRows $db "SELECT Action, Sequence FROM InstallExecuteSequence WHERE Action='CostFinalize'")"
[void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($db)
[void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($inst)
```
Expected: `CustomAction: SetARPINSTALLLOCATION | 51 | ARPINSTALLLOCATION | [INSTALLFOLDER]` (una sola fila; si WiX le da otro nombre a la acción, vale mientras `Source` y `Target` sean esos), `Secuencia: SetARPINSTALLLOCATION | <n>` y `CostFinalize: CostFinalize | 1000` con `<n>` mayor que 1000.

- [ ] **Step 8: Commit**

```powershell
git -C E:\notty add tools/release.ps1 installer/notty.wxs crates/notty-update/src/bin/notty_sign.rs .gitignore
git -C E:\notty commit -m "feat(release): notty.msi firmado como asset, --verify y ARPINSTALLLOCATION" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Perfil de pruebas que no toca el notty real (`NOTTY_PRUEBAS` + `tools/shot-settings.ps1`)

Por qué: lanzar `target\release\notty.exe` con un `APPDATA` vacío ejecuta `notepad::sync()` (escribe `HKCU\…\App Paths\notepad.exe`, `crates/notty-update/src/notepad.rs:44-56`), sirve el pipe de instancia única (`crates/notty/src/main.rs:169`) y llama a `global_hotkey::sync` (`main.rs:172`), que con la config por defecto (`mechanism = Daemon`, `start_with_windows = true`, `crates/notty-config/src/model.rs:349-353`) **sobrescribe `HKCU\…\Run\notty`** con la ruta de `target\release`, y con `lnk` **cierra el daemon real** y borra ese valor (`crates/notty-ui/src/global_hotkey.rs:40-79`).

**Files:**
- Modify: `crates/notty/src/main.rs:152-172`
- Modify: `crates/notty-ui/src/global_hotkey.rs:40`
- Create: `tools/shot-settings.ps1`

**Interfaces:**
- Produces:
  ```text
  NOTTY_PRUEBAS=1 → sin notepad::sync, sin servir ni usar \\.\pipe\notty-instance, global_hotkey::sync no hace nada
  tools/shot-settings.ps1 -Out <png> [-Steps 'click:x,y','key:VK','wait:ms'...] [-MainKeys '^0'...] [-Main] [-KeepOpen] [-ProcId <pid>]
      clics en DIPs de la ventana de Ajustes (lParam en píxeles = DIP × escala), teclas por WM_KEYDOWN/UP
      con -KeepOpen devuelve el PID por la salida; sin él cierra ese notty por su PID
  ```
  Rail de Ajustes (DIPs, ventana de 820×620): Apariencia `26,60`, Actualizaciones `26,250`. La captura es la ventana entera: píxel (px,py) de la imagen = DIP (px/escala, py/escala) (el script imprime la escala).

- [ ] **Step 1: `NOTTY_PRUEBAS` en `main.rs`**

En `E:\notty\crates\notty\src\main.rs` sustituye (líneas 152-172):
```rust
    if let Some(p) = &path {
        if cfg_for_check.files.open_in_existing_window {
            let msg = notty_ipc::Message::OpenPath(absolute_path_arg(p));
            if try_forward_to_existing_instance(&msg) {
                return Ok(());
            }
        }
    }

    // Cada usuario tiene su propio HKCU y el instalador solo corrió como uno de ellos.
    notty_update::notepad::sync();
    notty_ui::bench_log::mark("notepad_sync");

    // Esta instancia también escucha en el pipe mientras viva, además de abrir su
    // propia ventana con normalidad.
    let (tx, rx) = std::sync::mpsc::channel();
    let update_tx = tx.clone();
    spawn_pipe_server(tx);
```
por:
```rust
    // Perfil de pruebas (`tools/shot-settings.ps1`): ni registro, ni pipe de instancia
    // única, ni atajo global, para no pisar el notty de verdad del usuario.
    let pruebas = std::env::var_os("NOTTY_PRUEBAS").is_some();
    if let Some(p) = &path {
        if cfg_for_check.files.open_in_existing_window && !pruebas {
            let msg = notty_ipc::Message::OpenPath(absolute_path_arg(p));
            if try_forward_to_existing_instance(&msg) {
                return Ok(());
            }
        }
    }

    // Cada usuario tiene su propio HKCU y el instalador solo corrió como uno de ellos.
    if !pruebas {
        notty_update::notepad::sync();
    }
    notty_ui::bench_log::mark("notepad_sync");

    // Esta instancia también escucha en el pipe mientras viva, además de abrir su
    // propia ventana con normalidad.
    let (tx, rx) = std::sync::mpsc::channel();
    let update_tx = tx.clone();
    if !pruebas {
        spawn_pipe_server(tx);
    }
```
(Las líneas 171-172, `let hotkey = …` y el `thread::spawn` de `global_hotkey::sync`, se quedan: el guardia va dentro de `sync`.)

- [ ] **Step 2: `global_hotkey::sync` no hace nada en pruebas**

En `E:\notty\crates\notty-ui\src\global_hotkey.rs` sustituye:
```rust
pub fn sync(cfg: &HotkeyConfig) {
    {
```
por:
```rust
pub fn sync(cfg: &HotkeyConfig) {
    // Perfil de pruebas: ni `HKCU\…\Run` ni el daemon ni los `.lnk` del usuario.
    if std::env::var_os("NOTTY_PRUEBAS").is_some() {
        return;
    }
    {
```
(Cubre las dos llamadas: al arrancar, `main.rs:172`, y al guardar Ajustes, `settings_window.rs:1010`.)

- [ ] **Step 3: `tools/shot-settings.ps1`**

`E:\notty\tools\shot-settings.ps1` (contenido completo):
```powershell
# Lanza notty en el PERFIL DE PRUEBAS (APPDATA/LOCALAPPDATA en <repo>\.cache\appdata y
# NOTTY_PRUEBAS=1: sin registro, sin pipe de instancia única, sin atajo global), abre
# Ajustes con Ctrl+, , ejecuta los pasos pedidos sobre esa ventana y la captura por su HWND.
# Llámalo con `&` (no con `pwsh -File`) para poder recoger el PID de -KeepOpen.
#   & tools\shot-settings.ps1 -Out a.png -Steps 'click:26,250'          # Ajustes → Actualizaciones
#   & tools\shot-settings.ps1 -Out a.png -Main                          # solo la ventana principal
#   $id = & tools\shot-settings.ps1 -Out a.png -Main -KeepOpen          # deja notty abierto (PID)
#   & tools\shot-settings.ps1 -Out b.png -ProcId $id [-Main] [-KeepOpen] # otra captura de ese notty
# Pasos (en orden, sobre la ventana de Ajustes): 'click:x,y' (DIPs), 'key:VK' (decimal), 'wait:ms'.
# -MainKeys: teclas (sintaxis SendKeys) a la ventana principal antes de abrir Ajustes.
param(
  [Parameter(Mandatory)] [string]$Out,
  [string[]]$Steps = @(),
  [string[]]$MainKeys = @(),
  [switch]$Main,
  [switch]$KeepOpen,
  [int]$ProcId = 0
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target\release\notty.exe'
$prof = Join-Path $root '.cache\appdata'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
if (-not ('NottySet' -as [type])) {
  Add-Type @"
using System; using System.Runtime.InteropServices;
public class NottySet {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool fAttach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, string title);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
}
[NottySet]::SetProcessDPIAware() | Out-Null

function Force-Foreground([IntPtr]$h) {
  [NottySet]::ShowWindow($h, 9) | Out-Null  # SW_RESTORE
  $fg = [NottySet]::GetForegroundWindow()
  $myTid = [NottySet]::GetCurrentThreadId()
  [uint32]$fgPid = 0
  $fgTid = [NottySet]::GetWindowThreadProcessId($fg, [ref]$fgPid)
  if ($fgTid -ne 0) { [NottySet]::AttachThreadInput($myTid, $fgTid, $true) | Out-Null }
  [NottySet]::BringWindowToTop($h) | Out-Null
  [NottySet]::SetForegroundWindow($h) | Out-Null
  if ($fgTid -ne 0) { [NottySet]::AttachThreadInput($myTid, $fgTid, $false) | Out-Null }
}
function Focus([IntPtr]$h) {
  for ($i = 0; $i -lt 10; $i++) {
    Force-Foreground $h
    Start-Sleep -Milliseconds 200
    if ([NottySet]::GetForegroundWindow() -eq $h) { return }
  }
  Write-Warning 'No se consiguió el foco de forma fiable; la captura puede salir tapada.'
}
function Find-Settings([int]$procId) {
  $h = [IntPtr]::Zero
  while ($true) {
    $h = [NottySet]::FindWindowExW([IntPtr]::Zero, $h, 'NottySettingsClass', $null)
    if ($h -eq [IntPtr]::Zero) { return [IntPtr]::Zero }
    [uint32]$owner = 0
    [NottySet]::GetWindowThreadProcessId($h, [ref]$owner) | Out-Null
    if ($owner -eq $procId) { return $h }
  }
}

if ($ProcId -eq 0) {
  $open = @(Get-Process notty -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe })
  if ($open.Count -gt 0) { throw "Ya hay un notty de target\release abierto (PID $($open.Id -join ', ')): no es de este script, ciérralo tú." }
  $saved = @{ APPDATA = $env:APPDATA; LOCALAPPDATA = $env:LOCALAPPDATA; NOTTY_PRUEBAS = $env:NOTTY_PRUEBAS }
  try {
    [Environment]::SetEnvironmentVariable('APPDATA', "$prof\Roaming", 'Process')
    [Environment]::SetEnvironmentVariable('LOCALAPPDATA', "$prof\Local", 'Process')
    [Environment]::SetEnvironmentVariable('NOTTY_PRUEBAS', '1', 'Process')
    $p = Start-Process -FilePath $exe -PassThru
  } finally {
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process') }
  }
} else {
  $p = Get-Process -Id $ProcId
}

try {
  for ($i = 0; $i -lt 50 -and $p.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh() }
  $mainH = $p.MainWindowHandle
  if ($mainH -eq 0) { throw 'notty no abrió ninguna ventana' }
  Start-Sleep -Milliseconds 400
  foreach ($k in $MainKeys) {
    Focus $mainH
    [System.Windows.Forms.SendKeys]::SendWait($k)
    Start-Sleep -Milliseconds 300
  }
  $target = $mainH
  if (-not $Main) {
    $h = Find-Settings $p.Id
    if ($h -eq [IntPtr]::Zero) {
      Focus $mainH
      [System.Windows.Forms.SendKeys]::SendWait('^,')
      for ($i = 0; $i -lt 50 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 100; $h = Find-Settings $p.Id }
      if ($h -eq [IntPtr]::Zero) { throw 'No se abrió la ventana de Ajustes' }
      Start-Sleep -Milliseconds 600
    }
    $scale = [NottySet]::GetDpiForWindow($h) / 96.0
    Write-Host "escala $scale"
    foreach ($s in $Steps) {
      $kind, $arg = $s -split ':', 2
      switch ($kind) {
        'click' {
          $x, $y = $arg -split ',' | ForEach-Object { [double]$_ }
          $l = [IntPtr](([int]($y * $scale) -shl 16) -bor [int]($x * $scale))
          [NottySet]::PostMessageW($h, 0x201, [IntPtr]1, $l) | Out-Null  # WM_LBUTTONDOWN
          [NottySet]::PostMessageW($h, 0x202, [IntPtr]0, $l) | Out-Null  # WM_LBUTTONUP
        }
        'key' {
          [NottySet]::PostMessageW($h, 0x100, [IntPtr][int]$arg, [IntPtr]0) | Out-Null  # WM_KEYDOWN
          [NottySet]::PostMessageW($h, 0x101, [IntPtr][int]$arg, [IntPtr]0) | Out-Null  # WM_KEYUP
        }
        'wait' { Start-Sleep -Milliseconds ([int]$arg) }
        default { throw "Paso desconocido: $s" }
      }
      Start-Sleep -Milliseconds 500
    }
    $target = $h
  }
  Focus $target
  Start-Sleep -Milliseconds 900   # que terminen la entrada de página y los fundidos (350 ms)
  $r = New-Object NottySet+RECT
  [NottySet]::GetWindowRect($target, [ref]$r) | Out-Null
  $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
  New-Item -ItemType Directory -Force (Split-Path -Parent $Out) | Out-Null
  $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Host "$($r.R - $r.L)x$($r.B - $r.T) -> $Out"
} finally {
  if ($KeepOpen) { Write-Output $p.Id }
  elseif (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
```

- [ ] **Step 4: Compilar**

Run:
```powershell
Set-Location E:\notty
cargo build --release -p notty
```
Expected: `Finished `release` profile`.

- [ ] **Step 5: Perfil de pruebas y comprobación de que no toca nada real**

Run (en pwsh, desde `E:\notty`):
```powershell
$v = (Select-String -Path E:\notty\crates\notty\Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$cfgDir = "E:\notty\.cache\appdata\Roaming\notty"
New-Item -ItemType Directory -Force $cfgDir | Out-Null
Set-Content -LiteralPath "$cfgDir\config.toml" -Encoding utf8NoBOM -Value @"
first_run_done = true
last_seen_version = "$v"

[ui]
theme = "dark"

[hotkey]
mechanism = "lnk"

[updates]
check = false
last_check = 0
"@
$leer = { "$((Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).notty)|$((Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\notepad.exe' -ErrorAction SilentlyContinue).'(default)')" }
$antes = & $leer
$reales = @(Get-Process notty -ErrorAction SilentlyContinue | Where-Object { $_.Path -ne "E:\notty\target\release\notty.exe" } | Select-Object -ExpandProperty Id)
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t2-actualizaciones.png -Steps 'click:26,250'
"registro igual: $($antes -eq (& $leer))"
"reales siguen: $(@($reales | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }).Count) de $($reales.Count)"
"lnk de pruebas: $(Test-Path 'E:\notty\.cache\appdata\Roaming\Microsoft\Windows\Start Menu\Programs\notty')"
@(Get-Process notty -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq "E:\notty\target\release\notty.exe" }).Count
```
Expected: `escala …`, `820x620 -> …t2-actualizaciones.png` (multiplicado por la escala), `registro igual: True`, `reales siguen: N de N`, `lnk de pruebas: False` (con `mechanism = "lnk"`, un `sync` sin guardia los habría creado ahí) y `0` notty de `target\release` abiertos. Abre la captura con Read: Ajustes → Actualizaciones en oscuro, tarjeta «Sin comprobar en esta sesión» con «Buscar actualizaciones», «Buscar al abrir notty» apagado y «Firma verificada». Es la **referencia** de la tarjeta de estado para la Task 3.

- [ ] **Step 6: Commit**

```powershell
git -C E:\notty add crates/notty/src/main.rs crates/notty-ui/src/global_hotkey.rs tools/shot-settings.ps1
git -C E:\notty commit -m "test: perfil de pruebas que no toca el notty real (NOTTY_PRUEBAS) y captura de Ajustes" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `[updates] managed_by`: sin comprobar ni descargar, tarjeta «Gestionado por essentials»

**Files:**
- Modify: `crates/notty-config/src/model.rs:355-369` (`UpdatesConfig`)
- Modify: `crates/notty-config/src/storage.rs:41-46` (`save`, `save_as_is`)
- Modify: `crates/notty-config/src/lib.rs:172`
- Modify: `crates/notty/src/main.rs:241` (`spawn_update_check`)
- Modify: `crates/notty-ui/src/window.rs` (`open_settings_at` `:887-897`, `check_updates_now` `:1186-1190`, `start_update_download` `:1253-1254`)
- Modify: `crates/notty-ui/src/settings_model.rs:318` (`LinkAction`)
- Modify: `crates/notty-ui/src/settings_window.rs` (`State` `:83-124`, init `:255-275`, `shell_open` `:149`, `run_link` `:947-990`, `PageData` `:1199-1212`)
- Modify: `crates/notty-ui/src/settings_pages.rs` (`PageData` `:86-101`, `actualizaciones` `:1771-1779`, chip de Acerca de `:1998-2003`)
- Modify: `crates/notty-ui/src/strings.rs:481`

**Interfaces:**
- Produces:
  ```rust
  pub struct UpdatesConfig { pub check: bool, pub last_check: u64, pub managed_by: Option<String> } // ya no es Copy
  impl UpdatesConfig { pub fn managed(&self) -> bool }          // managed_by == Some("essentials")
  pub fn notty_config::save(cfg: &Config, path: &Path) -> io::Result<()>       // conserva managed_by del disco
  pub fn notty_config::save_as_is(cfg: &Config, path: &Path) -> io::Result<()> // tal cual (solo «Volver a gestionar yo…»)
  LinkAction::OpenEssentials, LinkAction::UnmanageUpdates
  PageData::essentials_missing: bool
  ```
- Riesgo del contrato cubierto así: notty no vigila `config.toml`; por eso `save` relee `managed_by` del disco antes de escribir (si essentials lo puso con notty abierto, ningún guardado lo borra) y Ajustes lo relee al abrirse (la tarjeta sale aunque notty llevara abierto desde antes).

- [ ] **Step 1: `UpdatesConfig::managed_by`**

En `E:\notty\crates\notty-config\src\model.rs` sustituye (líneas 355-369):
```rust
/// Si notty consulta (una vez al día, sin identificadores) si hay versión nueva
/// en GitHub. Apagado por defecto: la única forma de encenderlo son el paso
/// Privacidad de `welcome_window` o Ajustes → Acerca de.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesConfig {
    pub check: bool,
    pub last_check: u64,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self { check: false, last_check: 0 }
    }
}
```
por:
```rust
/// Si notty consulta (una vez al día, sin identificadores) si hay versión nueva
/// en GitHub. Apagado por defecto: la única forma de encenderlo son el paso
/// Privacidad de `welcome_window` o Ajustes → Acerca de.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesConfig {
    pub check: bool,
    pub last_check: u64,
    /// `"essentials"` cuando las actualizaciones las instala la tienda: lo escribe
    /// essentials al migrar; notty solo lo lee (y lo borra con «Volver a gestionar yo
    /// las actualizaciones»). Cualquier otro valor cuenta como si no estuviera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<String>,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self { check: false, last_check: 0, managed_by: None }
    }
}

impl UpdatesConfig {
    /// Las gestiona essentials: notty no comprueba ni descarga nada.
    pub fn managed(&self) -> bool {
        self.managed_by.as_deref() == Some("essentials")
    }
}
```

- [ ] **Step 2: `save` conserva `managed_by`; `save_as_is`**

En `E:\notty\crates\notty-config\src\storage.rs` sustituye (líneas 41-46):
```rust
pub fn save(cfg: &Config, path: &Path) -> std::io::Result<()> {
    let text = toml::to_string_pretty(cfg).expect("Config siempre serializa");
    backup_if_broken(path)?;
    notty_io::create_parent_dirs(path)?;
    notty_io::atomic_write(path, text.as_bytes())
}
```
por:
```rust
/// Guarda `cfg`. `[updates] managed_by` lo escribe essentials, no notty: se conserva el
/// que haya en disco, para que un guardado de notty (abierto desde antes) no lo borre.
pub fn save(cfg: &Config, path: &Path) -> std::io::Result<()> {
    let mut cfg = cfg.clone();
    if let LoadResult::Loaded(disk) = load(path) {
        cfg.updates.managed_by = disk.updates.managed_by;
    }
    save_as_is(&cfg, path)
}

/// Guarda `cfg` tal cual, `managed_by` incluido. Solo para «Volver a gestionar yo las
/// actualizaciones», que es lo único de notty que cambia esa clave.
pub fn save_as_is(cfg: &Config, path: &Path) -> std::io::Result<()> {
    let text = toml::to_string_pretty(cfg).expect("Config siempre serializa");
    backup_if_broken(path)?;
    notty_io::create_parent_dirs(path)?;
    notty_io::atomic_write(path, text.as_bytes())
}
```
En `E:\notty\crates\notty-config\src\lib.rs` sustituye `pub use storage::{LoadResult, default_path, load, save};` por:
```rust
pub use storage::{LoadResult, default_path, load, save, save_as_is};
```

- [ ] **Step 3: No comprobar ni descargar si está gestionado**

`E:\notty\crates\notty\src\main.rs`, en `spawn_update_check` sustituye:
```rust
    if !cfg.updates.check {
        return;
    }
```
por:
```rust
    if !cfg.updates.check || cfg.updates.managed() {
        return;
    }
```
`E:\notty\crates\notty-ui\src\window.rs`, en `check_updates_now` sustituye:
```rust
fn check_updates_now(w: &mut WindowState, hwnd: HWND) {
    if w.manual_check_rx.is_some() {
        return; // ya hay una en marcha
    }
```
por:
```rust
fn check_updates_now(w: &mut WindowState, hwnd: HWND) {
    if w.manual_check_rx.is_some() || w.cfg.borrow().updates.managed() {
        return; // ya hay una en marcha, o las actualizaciones son cosa de essentials
    }
```
y en `start_update_download` sustituye:
```rust
fn start_update_download(w: &mut WindowState, hwnd: HWND) {
    let Some(release) = w.update.available.clone() else { return };
```
por:
```rust
fn start_update_download(w: &mut WindowState, hwnd: HWND) {
    if w.cfg.borrow().updates.managed() {
        return;
    }
    let Some(release) = w.update.available.clone() else { return };
```
Y en `open_settings_at` sustituye:
```rust
    let cfg_for_settings = w.cfg.clone();
    let cfg_for_theme = w.cfg.clone();
```
por:
```rust
    // essentials pudo escribir `[updates] managed_by` con notty ya abierto.
    if let notty_config::LoadResult::Loaded(disk) = notty_config::load(&notty_config::default_path()) {
        w.cfg.borrow_mut().updates.managed_by = disk.updates.managed_by;
    }
    let cfg_for_settings = w.cfg.clone();
    let cfg_for_theme = w.cfg.clone();
```

- [ ] **Step 4: Acciones nuevas**

En `E:\notty\crates\notty-ui\src\settings_model.rs` sustituye (líneas 315-319):
```rust
    /// Ajustes es su propia ventana, no puede redimensionar la principal desde aquí).
    ResetWindowSize,
}
```
por:
```rust
    /// Ajustes es su propia ventana, no puede redimensionar la principal desde aquí).
    ResetWindowSize,
    /// Actualizaciones gestionadas → «Abrir en essentials» (`essentials.exe --app notty`).
    OpenEssentials,
    /// Actualizaciones gestionadas sin essentials → «Volver a gestionar yo las
    /// actualizaciones»: borra `[updates] managed_by`.
    UnmanageUpdates,
}
```

- [ ] **Step 5: `settings_window.rs`: estado, abrir essentials y las dos acciones**

En `E:\notty\crates\notty-ui\src\settings_window.rs`:

a) En `struct State`, sustituye:
```rust
    syn_pick: usize,
    syn_pick_at: Instant,
}
```
por:
```rust
    syn_pick: usize,
    syn_pick_at: Instant,
    /// «Abrir en essentials» falló (no hay `essentials.exe` registrado).
    essentials_missing: bool,
}
```
b) En la inicialización de `State` en `open`, sustituye:
```rust
            syn_pick: 0,
            syn_pick_at: Instant::now(),
        });
```
por:
```rust
            syn_pick: 0,
            syn_pick_at: Instant::now(),
            essentials_missing: false,
        });
```
c) Justo **encima** de `fn shell_open(hwnd: HWND, file: &str, params: Option<&str>) {` añade:
```rust
/// «Abrir en essentials»: `essentials.exe --app notty` por App Paths. Sin diálogo de
/// Windows si no está (`SEE_MASK_FLAG_NO_UI`): devuelve `false` y Ajustes lo dice.
fn open_essentials(hwnd: HWND) -> bool {
    use windows::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, ShellExecuteExW};
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        hwnd,
        lpVerb: w!("open"),
        lpFile: w!("essentials.exe"),
        lpParameters: w!("--app notty"),
        nShow: windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.is_ok()
}

```
d) En `run_link`, sustituye:
```rust
        LinkAction::ResetWindowSize => {
            {
                let mut cfg = st.cfg.borrow_mut();
                cfg.ui.win_w = notty_config::DEFAULT_WIN_W;
                cfg.ui.win_h = notty_config::DEFAULT_WIN_H;
                cfg.ui.win_maximized = false;
            }
            save_and_notify(st);
        }
    }
}
```
por:
```rust
        LinkAction::ResetWindowSize => {
            {
                let mut cfg = st.cfg.borrow_mut();
                cfg.ui.win_w = notty_config::DEFAULT_WIN_W;
                cfg.ui.win_h = notty_config::DEFAULT_WIN_H;
                cfg.ui.win_maximized = false;
            }
            save_and_notify(st);
        }
        LinkAction::OpenEssentials => st.essentials_missing = !open_essentials(hwnd),
        LinkAction::UnmanageUpdates => {
            st.cfg.borrow_mut().updates.managed_by = None;
            let _ = notty_config::save_as_is(&st.cfg.borrow(), &notty_config::default_path());
            st.essentials_missing = false;
            // Vuelve a entrar la página normal de Actualizaciones.
            st.page_enter = Instant::now();
            ensure_anim_timer(st, hwnd);
            (st.on_change)();
        }
    }
}
```
e) En `paint`, al construir `PageData`, sustituye:
```rust
        syn_pick_at: st.syn_pick_at,
        system_dark,
    };
```
por:
```rust
        syn_pick_at: st.syn_pick_at,
        system_dark,
        essentials_missing: st.essentials_missing,
    };
```

- [ ] **Step 6: `settings_pages.rs`: la tarjeta y el chip de Acerca de**

En `E:\notty\crates\notty-ui\src\settings_pages.rs`:

a) En `PageData`, sustituye:
```rust
    pub syn_pick: usize,
    pub syn_pick_at: Instant,
}
```
por:
```rust
    pub syn_pick: usize,
    pub syn_pick_at: Instant,
    /// «Abrir en essentials» falló: la tarjeta gestionada enseña el aviso y la salida.
    pub essentials_missing: bool,
}
```
b) Sustituye el principio de `actualizaciones` (líneas 1771-1779):
```rust
fn actualizaciones(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    let pal = ui.pal;
    let up = d.update;
    let version = crate::app_version();
    let newv = up.new_version.clone().unwrap_or_default();
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Actualizaciones", "") + 14.0;
    blk_end(ui);
```
por:
```rust
/// Actualizaciones cuando las gestiona essentials (`[updates] managed_by`): la misma
/// tarjeta de estado que la de siempre (de abajo), con «Abrir en essentials» y, si
/// essentials no está, el aviso en `danger` y «Volver a gestionar yo…».
fn managed_card(ui: &mut Ui, d: &PageData, x: f32, y: f32, w: f32) -> f32 {
    let pal = ui.pal;
    blk(ui, d, 1);
    let (sub, sub_c, label, primary, hit) = if d.essentials_missing {
        (ui.tr("essentials no está instalado"), pal.danger, ui.tr("Volver a gestionar yo las actualizaciones"), false, Hit::Link(LinkAction::UnmanageUpdates))
    } else {
        (ui.tr("Las actualizaciones de notty se instalan desde la tienda."), pal.text_2, ui.tr("Abrir en essentials"), true, Hit::Link(LinkAction::OpenEssentials))
    };
    let bw = ui.button_w(label, primary);
    let text_x = x + 22.0 + 56.0 + 18.0;
    let text_w = (x + w - 22.0 - bw - 18.0 - text_x).max(80.0);
    let head_h = 56.0;
    let card = Rect::new(x, y, x + w, y + 40.0 + head_h);
    ui.r.fill_round(card, 10.0, ui.row_c(0.0));
    let icx = x + 22.0 + 28.0;
    let icy = card.top + 20.0 + head_h / 2.0;
    ui.r.fill_circle(icx, icy, 28.0, pal.accent_soft);
    ui.icon(icon::ACTUALIZAR, icx, icy, 24.0, 2.0, pal.accent);
    let ty = card.top + 20.0 + (head_h - 22.0 - 3.0 - DESC_LH) / 2.0;
    ui.text(ui.tr("Gestionado por essentials"), 16.0, true, Rect::new(text_x, ty, text_x + text_w, ty + 22.0), pal.text);
    ui.text(sub, 12.0, false, Rect::new(text_x, ty + 25.0, text_x + text_w, ty + 25.0 + DESC_LH), sub_c);
    ui.button(Rect::new(x + w - 22.0 - bw, icy - 16.0, x + w - 22.0, icy + 16.0), label, primary, true, hit);
    blk_end(ui);
    card.bottom
}

fn actualizaciones(ui: &mut Ui, d: &PageData, x: f32, top: f32, w: f32) -> f32 {
    let cfg = d.cfg;
    let pal = ui.pal;
    let up = d.update;
    let version = crate::app_version();
    let newv = up.new_version.clone().unwrap_or_default();
    blk(ui, d, 0);
    let mut y = header(ui, x, top, w, "Actualizaciones", "") + 14.0;
    blk_end(ui);
    if cfg.updates.managed() {
        return managed_card(ui, d, x, y, w);
    }
```
c) En `acerca`, sustituye:
```rust
    let (st_txt, st_c) = match (&d.update.phase, d.update.up_to_date, &d.update.new_version) {
        (UpdatePhase::Found, _, Some(v)) => (format!("{}: {v}", ui.tr("Hay una versión nueva")), pal.accent),
```
por:
```rust
    let (st_txt, st_c) = match (&d.update.phase, d.update.up_to_date, &d.update.new_version) {
        _ if d.cfg.updates.managed() => (ui.tr("Gestionado por essentials").to_string(), pal.text_2),
        (UpdatePhase::Found, _, Some(v)) => (format!("{}: {v}", ui.tr("Hay una versión nueva")), pal.accent),
```

- [ ] **Step 7: Traducciones**

En `E:\notty\crates\notty-ui\src\strings.rs` sustituye (línea 481 y el cierre):
```rust
    ("No se pudo extraer el paquete a la carpeta temporal", "Couldn't extract the package to the temp folder"),
];
```
por:
```rust
    ("No se pudo extraer el paquete a la carpeta temporal", "Couldn't extract the package to the temp folder"),
    // --- settings_pages.rs: actualizaciones gestionadas por essentials ---
    ("Gestionado por essentials", "Managed by essentials"),
    ("Las actualizaciones de notty se instalan desde la tienda.", "notty updates are installed from the store."),
    ("Abrir en essentials", "Open in essentials"),
    ("essentials no está instalado", "essentials isn't installed"),
    ("Volver a gestionar yo las actualizaciones", "Manage updates myself again"),
];
```

- [ ] **Step 8: Compilar y tests existentes**

Run:
```powershell
Set-Location E:\notty
cargo build --release -p notty
cargo test -p notty-config -p notty-ui -p notty
```
Expected: `Finished`; todos los tests `ok` (si alguno no compila porque `UpdatesConfig` ya no es `Copy`, añade `.clone()` donde lo pida el compilador; no debería haber ninguno: hoy solo se leen sus campos).

- [ ] **Step 9: Prueba manual: con y sin `managed_by`, el fallo y la vuelta**

Antes, comprueba (solo lectura) que essentials no está registrado; si cualquiera da `True`/una ruta, **no** hagas los clics en «Abrir en essentials» (abrirían el essentials real) y anota «fallo no verificado: essentials instalado»:
```powershell
Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\essentials.exe'
Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\essentials.exe'
(Get-Command essentials.exe -ErrorAction SilentlyContinue).Source
```
Expected: `False`, `False`, nada.

Run (en pwsh, desde `E:\notty`; `$cfg` es la config del perfil de pruebas):
```powershell
$v = (Select-String -Path E:\notty\crates\notty\Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$cfg = "E:\notty\.cache\appdata\Roaming\notty\config.toml"
function Seed([string]$extra) {
  Set-Content -LiteralPath $cfg -Encoding utf8NoBOM -Value @"
first_run_done = true
last_seen_version = "$v"

[ui]
theme = "dark"

[updates]
check = true
last_check = 0
$extra
"@
}
# 1) Gestionado: tarjeta y sin comprobación al arrancar (last_check se queda en 0).
Seed 'managed_by = "essentials"'
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t3-gestionado.png -Steps 'click:26,250','wait:3000'
Select-String -LiteralPath $cfg -Pattern 'last_check|managed_by'
# 2) Acerca de con gestionado: el chip dice «Gestionado por essentials».
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t3-acerca.png -Steps 'click:26,288'
# 3) «Abrir en essentials» sin essentials → aviso en danger y «Volver a gestionar yo…».
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t3-fallo.png -Steps 'click:26,250','click:742,144'
# 4) «Volver a gestionar yo…» → página normal y la clave fuera del config.
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t3-vuelta.png -Steps 'click:26,250','click:742,144','click:742,144'
Select-String -LiteralPath $cfg -Pattern 'managed_by'; "managed_by tras volver: $([bool](Select-String -LiteralPath $cfg -Pattern 'managed_by' -Quiet))"
# 5) Sin managed_by y con check: la comprobación de arranque sí corre (last_check cambia).
Seed ''
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t3-normal.png -Steps 'click:26,250','wait:4000'
Select-String -LiteralPath $cfg -Pattern 'last_check'
# 6) essentials escribe managed_by con notty abierto; un guardado de notty no lo borra.
Seed ''
$id = & E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t3-antes.png -Main -KeepOpen
Add-Content -LiteralPath $cfg -Encoding utf8NoBOM -Value 'managed_by = "essentials"'
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t3-tras-guardar.png -ProcId $id -MainKeys '^0' -Steps 'click:26,250'
"reescrito: $([bool](Select-String -LiteralPath $cfg -Pattern '^\[files\]' -Quiet)) · managed_by: $([bool](Select-String -LiteralPath $cfg -Pattern 'managed_by = \"essentials\"' -Quiet))"
```
(`Add-Content` añade la línea al final del archivo, que acaba en la tabla `[updates]`; `^0` es Ctrl+0, que guarda la config entera con `set_font_scale`; la captura reabre Ajustes en ese mismo notty.)

Expected:
- 1) `t3-gestionado.png`: solo la cabecera «Actualizaciones» y una tarjeta `surface_2` de radio 10 con icono de actualizar en círculo `accent_soft`, «Gestionado por essentials» (16, seminegrita), «Las actualizaciones de notty se instalan desde la tienda.» (12, `text_2`) y el botón principal (acento) «Abrir en essentials» a la derecha; nada de «Buscar al abrir notty», «Firma verificada» ni versión. Mismas medidas que la tarjeta de `t2-actualizaciones.png`. `Select-String` muestra `last_check = 0` y `managed_by = "essentials"`.
- 2) `t3-acerca.png`: el chip junto a la versión dice «Gestionado por essentials» con punto `text_2`; el enlace «Novedades» sigue en la lista.
- 3) `t3-fallo.png`: la misma tarjeta con «essentials no está instalado» en `danger` (rojo) y el botón secundario (`chrome`) «Volver a gestionar yo las actualizaciones»; no sale ningún diálogo de Windows.
- 4) `t3-vuelta.png`: la página normal (tarjeta «Sin comprobar en esta sesión», «Buscar al abrir notty» encendido…); `managed_by tras volver: False`.
- 5) `last_check = <número distinto de 0>`.
- 6) `reescrito: True · managed_by: True` y `t3-tras-guardar.png` con la tarjeta «Gestionado por essentials».

Si un clic no cae en su sitio (otra escala o una ventana de Ajustes recortada por la pantalla), abre la captura, localiza el botón y recalcula su centro en DIPs (`px / escala`).

- [ ] **Step 10: Commit**

```powershell
git -C E:\notty add crates/notty-config/src/model.rs crates/notty-config/src/storage.rs crates/notty-config/src/lib.rs crates/notty/src/main.rs crates/notty-ui/src/window.rs crates/notty-ui/src/settings_model.rs crates/notty-ui/src/settings_window.rs crates/notty-ui/src/settings_pages.rs crates/notty-ui/src/strings.rs
git -C E:\notty commit -m "feat(updates): managed_by = essentials apaga el actualizador y Ajustes enseña «Gestionado por essentials»" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Tema compartido: leer `appearance.toml`, seguirlo y escribirlo

**Files:**
- Create: `crates/notty-config/src/appearance.rs`
- Modify: `crates/notty-config/src/lib.rs`, `crates/notty-config/src/model.rs` (`UiConfig` `:149-206`, `Default` `:213-244`)
- Create: `crates/notty-ui/src/shared_theme.rs`
- Modify: `crates/notty-ui/src/lib.rs:31`
- Modify: `crates/notty-ui/src/window.rs` (`render_ui` `:188`, `view_state` `:198`, callbacks `:901`/`:1927`, `run_inner` `:1724`/`:1797`, `WM_PAINT` `:2111`/`:2131`, `WM_SETTINGCHANGE` `:2368`)
- Modify: `crates/notty-ui/src/settings_window.rs` (`:222`, `:380`, `Hit::Toggle` `:845-849`, `set_value` `:991-1006`, `paint` `:1139-1140`)
- Modify: `crates/notty-ui/src/settings_model.rs` (`SettingKey` `:144`, `apply` `:482-485`, `current_bool` `:522`, `selected_index` `:532`/`:540`)
- Modify: `crates/notty-ui/src/settings_pages.rs` (Apariencia `:389-393`)
- Modify: `crates/notty-ui/src/strings.rs`

**Interfaces:**
- Produces:
  ```rust
  // notty-config (puro, sin Windows)
  pub fn appearance_path() -> Option<PathBuf>;                                   // %APPDATA%\essentials\appearance.toml
  pub fn read_appearance(path: &Path) -> Option<(Theme, AccentColor)>;          // None si falta, no parsea o shared != true
  pub fn write_appearance(path: &Path, theme: Theme, accent: AccentColor) -> io::Result<()>; // solo theme/accent, .tmp + rename
  pub struct UiConfig { …, pub follow_essentials: bool }                         // [ui] follow_essentials, por defecto true
  // notty-ui
  pub(crate) fn shared_theme::reload() -> bool;            // relee; true si cambió (existe / valores)
  pub(crate) fn shared_theme::file_exists() -> bool;
  pub(crate) fn shared_theme::effective(ui: &UiConfig) -> (Theme, AccentColor);
  pub(crate) fn shared_theme::push(ui: &UiConfig, theme: Option<Theme>, accent: Option<AccentColor>);
  SettingKey::FollowEssentials
  ```
- Mapeos: `theme` `sistema/claro/oscuro` ↔ `Theme::System/Light/Dark`; `accent` `sistema` ↔ `AccentColor::Azul` (el azul de cada paleta, `theme.rs:160-163`), el resto por nombre (`verde` ↔ `Verde`…).
- Regla: «efectivo» = los de essentials si `follow_essentials` y el archivo existe, parsea y `shared = true`; si no, los propios de `[ui]`. La config propia **nunca** se sobrescribe con los de essentials (apagar el interruptor devuelve los suyos). Elegir Tema o Acento en Ajustes cambia el propio como siempre y, siguiendo a essentials, escribe ese componente en `appearance.toml` (el otro se queda como estaba en el archivo).

- [ ] **Step 1: `notty-config/src/appearance.rs`**

`E:\notty\crates\notty-config\src\appearance.rs` (contenido completo):
```rust
//! `%APPDATA%\essentials\appearance.toml`: el tema y el acento que essentials comparte
//! con sus apps (`theme`, `accent`, `shared`). Lo escribe essentials; notty lo lee y,
//! si lo sigue, cambia solo `theme` y `accent` cuando el usuario los toca en notty.

use std::path::{Path, PathBuf};

use crate::{AccentColor, Theme};

/// `%APPDATA%\essentials\appearance.toml`.
pub fn appearance_path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("essentials").join("appearance.toml"))
}

/// Nombre de cada acento en `appearance.toml`. «sistema» es el `Azul` de siempre.
const ACCENTS: [(&str, AccentColor); 8] = [
    ("sistema", AccentColor::Azul),
    ("verde", AccentColor::Verde),
    ("turquesa", AccentColor::Turquesa),
    ("morado", AccentColor::Morado),
    ("rosa", AccentColor::Rosa),
    ("rojo", AccentColor::Rojo),
    ("naranja", AccentColor::Naranja),
    ("amarillo", AccentColor::Amarillo),
];

const THEMES: [(&str, Theme); 3] = [("sistema", Theme::System), ("claro", Theme::Light), ("oscuro", Theme::Dark)];

/// Tema y acento del archivo, o `None` si no existe, no se entiende o `shared` no es `true`.
pub fn read_appearance(path: &Path) -> Option<(Theme, AccentColor)> {
    let table: toml::Table = toml::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if table.get("shared")?.as_bool() != Some(true) {
        return None;
    }
    let theme = table.get("theme")?.as_str()?;
    let accent = table.get("accent")?.as_str()?;
    let theme = THEMES.iter().find(|(n, _)| *n == theme)?.1;
    let accent = ACCENTS.iter().find(|(n, _)| *n == accent)?.1;
    Some((theme, accent))
}

/// Cambia `theme` y `accent` del archivo conservando el resto (`shared`…), con
/// temporal + rename en la misma carpeta, como essentials.
pub fn write_appearance(path: &Path, theme: Theme, accent: AccentColor) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut table: toml::Table = toml::from_str(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let theme = THEMES.iter().find(|(_, t)| *t == theme).map_or("sistema", |(n, _)| *n);
    let accent = ACCENTS.iter().find(|(_, a)| *a == accent).map_or("sistema", |(n, _)| *n);
    table.insert("theme".to_string(), toml::Value::String(theme.to_string()));
    table.insert("accent".to_string(), toml::Value::String(accent.to_string()));
    let tmp = path.with_file_name("appearance.toml.tmp");
    std::fs::write(&tmp, toml::to_string(&table).expect("una tabla siempre serializa"))?;
    std::fs::rename(&tmp, path)
}
```
En `E:\notty\crates\notty-config\src\lib.rs` sustituye:
```rust
mod model;
mod preset;
mod storage;
```
por:
```rust
mod appearance;
mod model;
mod preset;
mod storage;

pub use appearance::{appearance_path, read_appearance, write_appearance};
```

- [ ] **Step 2: `UiConfig::follow_essentials`**

En `E:\notty\crates\notty-config\src\model.rs`, en `struct UiConfig` sustituye:
```rust
    /// Ajustes → Archivos → qué mecanismo de Previsualización usa un `.md`.
    pub md_preview_style: MdPreviewStyle,
}
```
por:
```rust
    /// Ajustes → Archivos → qué mecanismo de Previsualización usa un `.md`.
    pub md_preview_style: MdPreviewStyle,
    /// Ajustes → Apariencia → «Seguir el tema de essentials»: con
    /// `%APPDATA%\essentials\appearance.toml` (`shared = true`), su tema y su acento
    /// mandan sobre `theme`/`accent` (que no se tocan). Ver `notty_ui::shared_theme`.
    pub follow_essentials: bool,
}
```
y en `impl Default for UiConfig` sustituye:
```rust
            md_preview_style: MdPreviewStyle::default(),
        };
```
por:
```rust
            md_preview_style: MdPreviewStyle::default(),
            follow_essentials: true,
        };
```

- [ ] **Step 3: `notty-ui/src/shared_theme.rs` (sin vigilancia todavía)**

`E:\notty\crates\notty-ui\src\shared_theme.rs` (contenido completo; la Task 5 le añade `watch`):
```rust
//! Tema compartido con essentials (`%APPDATA%\essentials\appearance.toml`, ver
//! `notty_config::read_appearance`): si existe con `shared = true` y `[ui]
//! follow_essentials` está activo, su tema y su acento mandan sobre los de notty.
//! Lo que hay en el archivo vive aquí (un proceso, una copia); `effective` es lo que
//! se pinta y la ventana lo lee donde antes leía `cfg.ui.theme`/`cfg.ui.accent`.

use std::sync::{Mutex, MutexGuard};

use notty_config::{AccentColor, Theme, UiConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shared {
    /// El archivo existe (aunque no se entienda): enseña la fila de Ajustes.
    exists: bool,
    /// Sus valores, si se entiende y `shared = true`.
    values: Option<(Theme, AccentColor)>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { exists: false, values: None });

fn lock() -> MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Relee el archivo. `true` si cambió algo (que exista o sus valores).
pub(crate) fn reload() -> bool {
    let new = match notty_config::appearance_path() {
        Some(p) => Shared { exists: p.exists(), values: notty_config::read_appearance(&p) },
        None => Shared { exists: false, values: None },
    };
    let mut s = lock();
    let changed = *s != new;
    *s = new;
    changed
}

/// Si existe `appearance.toml` (la fila «Seguir el tema de essentials» solo sale entonces).
pub(crate) fn file_exists() -> bool {
    lock().exists
}

/// Tema y acento que se pintan: los de essentials si se siguen y están, si no los de `ui`.
pub(crate) fn effective(ui: &UiConfig) -> (Theme, AccentColor) {
    match lock().values {
        Some(v) if ui.follow_essentials => v,
        _ => (ui.theme, ui.accent),
    }
}

/// El usuario eligió tema o acento en notty: si se sigue a essentials, ese componente
/// se escribe en `appearance.toml` (así cambia en todas sus apps).
pub(crate) fn push(ui: &UiConfig, theme: Option<Theme>, accent: Option<AccentColor>) {
    if !ui.follow_essentials {
        return;
    }
    let mut s = lock();
    let Some((t0, a0)) = s.values else { return };
    let new = (theme.unwrap_or(t0), accent.unwrap_or(a0));
    let Some(path) = notty_config::appearance_path() else { return };
    if notty_config::write_appearance(&path, new.0, new.1).is_ok() {
        s.values = Some(new);
    }
}
```
En `E:\notty\crates\notty-ui\src\lib.rs` sustituye `pub mod settings_window;` por:
```rust
pub mod settings_window;
mod shared_theme;
```

- [ ] **Step 4: `window.rs` lee el tema efectivo**

En `E:\notty\crates\notty-ui\src\window.rs`:

a) `render_ui` — sustituye:
```rust
    fn render_ui(&self) -> notty_config::UiConfig {
        let mut ui = self.cfg.borrow().ui;
        if ui.menubar == notty_config::MenuBar::Alt {
```
por:
```rust
    fn render_ui(&self) -> notty_config::UiConfig {
        let mut ui = self.cfg.borrow().ui;
        // Siguiendo a essentials, su tema y su acento en vez de los propios.
        (ui.theme, ui.accent) = crate::shared_theme::effective(&ui);
        if ui.menubar == notty_config::MenuBar::Alt {
```
b) `view_state` — sustituye:
```rust
        let dark = crate::is_dark(self.cfg.borrow().ui.theme, system_uses_dark_mode());
        let maximized = unsafe { IsZoomed(hwnd).as_bool() };
```
por:
```rust
        let dark = crate::is_dark(self.render_ui().theme, system_uses_dark_mode());
        let maximized = unsafe { IsZoomed(hwnd).as_bool() };
```
c) Las **dos** apariciones (líneas 901 y 1927) de
```rust
crate::is_dark(cfg_for_theme.borrow().ui.theme, system_uses_dark_mode())
```
por (Edit con `replace_all`):
```rust
crate::is_dark(crate::shared_theme::effective(&cfg_for_theme.borrow().ui).0, system_uses_dark_mode())
```
d) En `run_inner`, sustituye:
```rust
    crate::anim::configure(&cfg.ui);
    let run_lang = crate::lang::resolve(cfg.ui.lang);
```
por:
```rust
    crate::anim::configure(&cfg.ui);
    crate::shared_theme::reload();
    let run_lang = crate::lang::resolve(cfg.ui.lang);
```
y sustituye:
```rust
        let dark = crate::is_dark(cfg.ui.theme, system_uses_dark_mode());
        setup_chrome(hwnd, dark);
```
por:
```rust
        let dark = crate::is_dark(crate::shared_theme::effective(&cfg.ui).0, system_uses_dark_mode());
        setup_chrome(hwnd, dark);
```
e) En `WM_PAINT` (`ui` es el `w.render_ui()` de la línea 2058), sustituye:
```rust
                    let dark =crate::is_dark(w.cfg.borrow().ui.theme, system_uses_dark_mode());
```
por:
```rust
                    let dark = crate::is_dark(ui.theme, system_uses_dark_mode());
```
y sustituye:
```rust
                    let accent = w.cfg.borrow().ui.accent;
```
por:
```rust
                    let accent = ui.accent;
```
f) En `WM_SETTINGCHANGE`, sustituye:
```rust
                    let dark = crate::is_dark(w.cfg.borrow().ui.theme, system_uses_dark_mode());
                    apply_dark_mode(hwnd, dark);
```
por:
```rust
                    let dark = crate::is_dark(w.render_ui().theme, system_uses_dark_mode());
                    apply_dark_mode(hwnd, dark);
```

- [ ] **Step 5: `settings_model.rs`: la clave y el valor elegido efectivo**

En `E:\notty\crates\notty-ui\src\settings_model.rs`:

a) En `enum SettingKey`, sustituye:
```rust
    AnimHz,
    ReducedMotion,
}
```
por:
```rust
    AnimHz,
    ReducedMotion,
    FollowEssentials,
}
```
b) En `apply`, sustituye:
```rust
        (SettingKey::ReducedMotion, SettingValue::Bool(b)) => {
            cfg.ui.reduced_motion = b;
            return;
        }
```
por:
```rust
        (SettingKey::ReducedMotion, SettingValue::Bool(b)) => {
            cfg.ui.reduced_motion = b;
            return;
        }
        (SettingKey::FollowEssentials, SettingValue::Bool(b)) => {
            cfg.ui.follow_essentials = b;
            return;
        }
```
c) En `current_bool`, sustituye:
```rust
        SettingKey::ReducedMotion => cfg.ui.reduced_motion,
        _ => false,
```
por:
```rust
        SettingKey::ReducedMotion => cfg.ui.reduced_motion,
        SettingKey::FollowEssentials => cfg.ui.follow_essentials,
        _ => false,
```
d) En `selected_index`, sustituye:
```rust
        SettingKey::Theme => SettingValue::Theme(cfg.ui.theme),
```
por:
```rust
        SettingKey::Theme => SettingValue::Theme(crate::shared_theme::effective(&cfg.ui).0),
```
y sustituye:
```rust
        SettingKey::AccentColor => SettingValue::AccentColor(cfg.ui.accent),
        SettingKey::Lang => SettingValue::Lang(cfg.ui.lang),
```
por:
```rust
        SettingKey::AccentColor => SettingValue::AccentColor(crate::shared_theme::effective(&cfg.ui).1),
        SettingKey::Lang => SettingValue::Lang(cfg.ui.lang),
```

- [ ] **Step 6: `settings_window.rs`: tema efectivo, escribir a essentials y fundido**

En `E:\notty\crates\notty-ui\src\settings_window.rs`:

a) En `open`, sustituye:
```rust
        let dark = is_dark(cfg.borrow().ui.theme, crate::window::system_uses_dark_mode());
        let prefer_round = DWMWCP_ROUND;
```
por:
```rust
        let dark = is_dark(crate::shared_theme::effective(&cfg.borrow().ui).0, crate::window::system_uses_dark_mode());
        let prefer_round = DWMWCP_ROUND;
```
b) En `WM_SETTINGCHANGE`, sustituye:
```rust
                    let dark = is_dark(st.cfg.borrow().ui.theme, crate::window::system_uses_dark_mode());
```
por:
```rust
                    let dark = is_dark(crate::shared_theme::effective(&st.cfg.borrow().ui).0, crate::window::system_uses_dark_mode());
```
c) En `handle_click`, sustituye:
```rust
        Hit::Toggle(key) => {
            let v = !model::current_bool(&st.cfg.borrow(), key);
            model::apply(&mut st.cfg.borrow_mut(), key, SettingValue::Bool(v));
            save_and_notify(st);
        }
```
por:
```rust
        Hit::Toggle(key) => {
            // Por `set_value`: «Seguir el tema de essentials» también cambia el tema.
            let v = !model::current_bool(&st.cfg.borrow(), key);
            set_value(st, hwnd, key, SettingValue::Bool(v));
        }
```
d) Sustituye `set_value` entero:
```rust
/// Aplica un valor elegido (selector, tarjeta, fuente) y, si cambió el tema, arranca
/// el fundido y avisa a DWM para que el marco nativo lo siga.
fn set_value(st: &mut State, hwnd: HWND, key: SettingKey, value: SettingValue) {
    let system_dark = crate::window::system_uses_dark_mode();
    let was_dark = is_dark(st.cfg.borrow().ui.theme, system_dark);
    let was_accent = st.cfg.borrow().ui.accent;
    model::apply(&mut st.cfg.borrow_mut(), key, value);
    save_and_notify(st);
    let now_dark = is_dark(st.cfg.borrow().ui.theme, system_dark);
    if now_dark != was_dark {
        st.theme_from = Some((was_dark, Instant::now()));
        unsafe { crate::window::apply_dark_mode(hwnd, now_dark) };
    }
    let now_accent = st.cfg.borrow().ui.accent;
    if now_accent != was_accent {
        st.accent_from = Some((was_accent, Instant::now()));
    }
}
```
por:
```rust
/// Aplica un valor elegido (selector, tarjeta, fuente, interruptor) y, si cambió el tema
/// o el acento que se ve, arranca el fundido y avisa a DWM para que el marco lo siga.
/// Siguiendo a essentials, el tema o el acento elegidos se le escriben también a él.
fn set_value(st: &mut State, hwnd: HWND, key: SettingKey, value: SettingValue) {
    let (was_theme, was_accent) = crate::shared_theme::effective(&st.cfg.borrow().ui);
    let was_dark = is_dark(was_theme, crate::window::system_uses_dark_mode());
    model::apply(&mut st.cfg.borrow_mut(), key, value);
    match value {
        SettingValue::Theme(t) => crate::shared_theme::push(&st.cfg.borrow().ui, Some(t), None),
        SettingValue::AccentColor(c) => crate::shared_theme::push(&st.cfg.borrow().ui, None, Some(c)),
        _ => {}
    }
    save_and_notify(st);
    start_theme_fade(st, hwnd, was_dark, was_accent);
}

/// Fundido de 350 ms desde lo que se veía (`was_*`) hasta el tema/acento efectivos.
fn start_theme_fade(st: &mut State, hwnd: HWND, was_dark: bool, was_accent: notty_config::AccentColor) {
    let (theme, accent) = crate::shared_theme::effective(&st.cfg.borrow().ui);
    let now_dark = is_dark(theme, crate::window::system_uses_dark_mode());
    if now_dark != was_dark {
        st.theme_from = Some((was_dark, Instant::now()));
        unsafe { crate::window::apply_dark_mode(hwnd, now_dark) };
    }
    if accent != was_accent {
        st.accent_from = Some((was_accent, Instant::now()));
    }
}
```
e) En `paint`, sustituye:
```rust
    let accent = st.cfg.borrow().ui.accent;
    let dark = is_dark(st.cfg.borrow().ui.theme, system_dark);
```
por:
```rust
    let (theme_now, accent) = crate::shared_theme::effective(&st.cfg.borrow().ui);
    let dark = is_dark(theme_now, system_dark);
```

- [ ] **Step 7: Fila «Seguir el tema de essentials» en Apariencia**

En `E:\notty\crates\notty-ui\src\settings_pages.rs`, en `apariencia`, sustituye:
```rust
    blk(ui, d, 4);
    y = label(ui, x, y, w, "Color de acento") + 8.0;
    let sel = model::selected_index(cfg, SettingKey::AccentColor, model::ACCENT_OPTS);
    y = cards(ui, x, y, w, SettingKey::AccentColor, sel, accent_thumb) + 16.0;
    blk_end(ui);
```
por:
```rust
    blk(ui, d, 4);
    y = label(ui, x, y, w, "Color de acento") + 8.0;
    let sel = model::selected_index(cfg, SettingKey::AccentColor, model::ACCENT_OPTS);
    y = cards(ui, x, y, w, SettingKey::AccentColor, sel, accent_thumb) + 16.0;
    if crate::shared_theme::file_exists() {
        let desc = "Usa el tema y el color de acento de essentials; cambiarlos aquí también los cambia allí.";
        y = toggle_row(ui, x, y, w, cfg, SettingKey::FollowEssentials, "Seguir el tema de essentials", desc) + 16.0;
    }
    blk_end(ui);
```

- [ ] **Step 8: Traducciones**

En `E:\notty\crates\notty-ui\src\strings.rs` sustituye:
```rust
    ("Volver a gestionar yo las actualizaciones", "Manage updates myself again"),
];
```
por:
```rust
    ("Volver a gestionar yo las actualizaciones", "Manage updates myself again"),
    // --- settings_pages.rs: Apariencia, tema compartido con essentials ---
    ("Seguir el tema de essentials", "Follow the essentials theme"),
    ("Usa el tema y el color de acento de essentials; cambiarlos aquí también los cambia allí.", "Uses the essentials theme and accent color; changing them here changes them there too."),
];
```

- [ ] **Step 9: Compilar y tests existentes**

Run:
```powershell
Set-Location E:\notty
cargo build --release -p notty
cargo test -p notty-config -p notty-ui
```
Expected: `Finished`; tests `ok`.

- [ ] **Step 10: Prueba manual: seguir, apagar, escribir y sin archivo**

Run (en pwsh, desde `E:\notty`):
```powershell
$v = (Select-String -Path E:\notty\crates\notty\Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$cfg = "E:\notty\.cache\appdata\Roaming\notty\config.toml"
$ess = "E:\notty\.cache\appdata\Roaming\essentials"
$app = "$ess\appearance.toml"
New-Item -ItemType Directory -Force $ess | Out-Null
Set-Content -LiteralPath $cfg -Encoding utf8NoBOM -Value @"
first_run_done = true
last_seen_version = "$v"

[ui]
theme = "light"
accent = "rojo"
"@
Set-Content -LiteralPath $app -Encoding utf8NoBOM -Value @"
theme = "oscuro"
accent = "morado"
shared = true
"@
# 1) Sigue a essentials: ventana principal oscura con acento morado.
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t4-sigue-main.png -Main
# 2) Ajustes → Apariencia: Tema «Oscuro» y Acento «Morado» marcados; y la fila del interruptor (bajando con Fin).
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t4-apariencia.png
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t4-fila.png -Steps 'key:35'
```
Expected: `t4-sigue-main.png` en oscuro con el logo y los resaltados en morado (aunque `[ui]` dice `light`/`rojo`); `t4-apariencia.png` con la tarjeta «Oscuro» y el círculo «Morado» seleccionados; `t4-fila.png` con, debajo de los acentos, la fila `surface_2` «Seguir el tema de essentials» / su descripción (12, `text_2`) y el interruptor encendido (acento), con el mismo aspecto que «Números de línea» más abajo.

Ahora abre `t4-fila.png` con Read, localiza el interruptor de esa fila y calcula su centro en DIPs (`px / escala`; en una ventana de 820 DIPs está en x ≈ 750). Con ese `X,Y` y la tarjeta «Claro» de `t4-apariencia.png` (en x ≈ 436 DIPs; su `Y` también de la captura):
```powershell
# 3) Elegir «Claro» en notty siguiendo a essentials: escribe theme = "claro" (y conserva accent y shared).
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t4-claro.png -Steps 'click:436,<Y de Claro>'
Get-Content -LiteralPath $app; "temporal sobrante: $(Test-Path "$ess\appearance.toml.tmp")"
Select-String -LiteralPath $cfg -Pattern '^theme|^accent'
# 4) Apagar el interruptor: vuelve a los suyos (claro + rojo) y lo guarda.
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t4-apagado.png -Steps 'key:35','click:<X>,<Y>'
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t4-apagado-main.png -Main
Select-String -LiteralPath $cfg -Pattern 'follow_essentials'
# 5) Sin appearance.toml: sin fila y con el tema propio.
Remove-Item -LiteralPath "E:\notty\.cache\appdata\Roaming\essentials\appearance.toml"
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t4-sin-archivo.png -Steps 'key:35'
```
Expected:
- 3) `appearance.toml` con `theme = "claro"`, `accent = "morado"` y `shared = true` (el orden de las claves puede cambiar), `temporal sobrante: False`; en `config.toml` el propio pasa a `theme = "light"` (lo era) y `accent = "rojo"` se queda; `t4-claro.png` en claro y morado.
- 4) `t4-apagado.png` con el interruptor apagado y la ventana en claro con acento rojo; `t4-apagado-main.png` igual; `follow_essentials = false` en el config.
- 5) `t4-sin-archivo.png` sin la fila «Seguir el tema de essentials» (justo después de los acentos viene «Fuente»).

- [ ] **Step 11: Commit**

```powershell
git -C E:\notty add crates/notty-config/src/appearance.rs crates/notty-config/src/lib.rs crates/notty-config/src/model.rs crates/notty-ui/src/shared_theme.rs crates/notty-ui/src/lib.rs crates/notty-ui/src/window.rs crates/notty-ui/src/settings_window.rs crates/notty-ui/src/settings_model.rs crates/notty-ui/src/settings_pages.rs crates/notty-ui/src/strings.rs
git -C E:\notty commit -m "feat(tema): sigue el tema y el acento de essentials (appearance.toml) y se los escribe" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Cambio en caliente: `ReadDirectoryChangesW` y fundido de 350 ms

**Files:**
- Modify: `crates/notty-ui/Cargo.toml` (features de `windows`)
- Modify: `crates/notty-ui/src/shared_theme.rs` (añadir `watch`)
- Modify: `crates/notty-ui/src/window.rs` (`WM_SHARED_THEME` junto a `WM_IPC` `:44-55`, arranque `:1907`, `wndproc` junto a `WM_IPC =>` `:3085`)
- Modify: `crates/notty-ui/src/settings_window.rs` (añadir `shared_theme_changed` tras `focus_existing` `:809-820`)

**Interfaces:**
- Consumes: `shared_theme::reload/effective` (Task 4), `start_theme_fade` (Task 4).
- Produces:
  ```rust
  pub(crate) fn shared_theme::watch(on_change: fn());   // hilo; solo si %APPDATA%\essentials existe al arrancar
  pub fn settings_window::shared_theme_changed(hwnd: HWND, was_dark: bool, was_accent: AccentColor);
  const WM_SHARED_THEME: u32 = WM_APP + 10;            // window.rs
  ```
- La ventana principal no necesita código de fundido nuevo: su `WM_PAINT` ya compara `last_dark`/`last_accent` con lo que pinta y funde 350 ms (`window.rs:2111-2149`); Ajustes, que solo fundía al elegir, recibe `shared_theme_changed`.

- [ ] **Step 1: Features**

En `E:\notty\crates\notty-ui\Cargo.toml`, en la lista de features de `windows`, sustituye `"Win32_Globalization", "Win32_Media"] }` por:
```toml
"Win32_Globalization", "Win32_Media", "Win32_Storage_FileSystem", "Win32_System_IO", "Win32_Security"] }
```

- [ ] **Step 2: `watch` en `shared_theme.rs`**

En `E:\notty\crates\notty-ui\src\shared_theme.rs`, sustituye:
```rust
use std::sync::{Mutex, MutexGuard};

use notty_config::{AccentColor, Theme, UiConfig};
```
por:
```rust
use std::sync::{Mutex, MutexGuard};

use notty_config::{AccentColor, Theme, UiConfig};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows::core::HSTRING;
```
y añade al final del archivo:
```rust
/// Vigila `%APPDATA%\essentials\` en un hilo (`ReadDirectoryChangesW`) y llama a
/// `on_change` tras cada cambio; quien lo recibe relee con `reload`. Solo si la carpeta
/// existe al arrancar (sin essentials no hay nada que vigilar).
pub(crate) fn watch(on_change: fn()) {
    let Some(dir) = notty_config::appearance_path().and_then(|p| p.parent().map(|d| d.to_path_buf())) else { return };
    if !dir.is_dir() {
        return;
    }
    std::thread::spawn(move || unsafe {
        let Ok(h) = CreateFileW(
            &HSTRING::from(dir.as_os_str()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        ) else {
            return;
        };
        let mut buf = [0u32; 1024];
        loop {
            let mut got = 0u32;
            if ReadDirectoryChangesW(
                h,
                buf.as_mut_ptr().cast(),
                std::mem::size_of_val(&buf) as u32,
                false,
                FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME,
                Some(&mut got),
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            // Un guardado son varios avisos seguidos (temporal, rename…): se deja acabar.
            std::thread::sleep(std::time::Duration::from_millis(60));
            on_change();
        }
        let _ = CloseHandle(h);
    });
}
```

- [ ] **Step 3: `window.rs`: mensaje, arranque del vigilante y manejador**

En `E:\notty\crates\notty-ui\src\window.rs`:

a) Sustituye:
```rust
/// Id del `SetTimer` de animación (60Hz): repinta mientras haya alguna animación en
```
por:
```rust
/// Lo manda el hilo que vigila `%APPDATA%\essentials\` (`shared_theme::watch`).
const WM_SHARED_THEME: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 10;

/// Avisa a la ventana de que `appearance.toml` de essentials pudo cambiar.
fn wake_for_shared_theme() {
    let h = IPC_HWND.load(std::sync::atomic::Ordering::Acquire);
    if h != 0 {
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(HWND(h as *mut _)), WM_SHARED_THEME, WPARAM(0), LPARAM(0)) };
    }
}

/// Id del `SetTimer` de animación (60Hz): repinta mientras haya alguna animación en
```
b) Sustituye:
```rust
        IPC_HWND.store(hwnd.0 as usize, std::sync::atomic::Ordering::Release);
        // Lo que llegase antes de registrar la ventana.
        wake_for_ipc();
```
por:
```rust
        IPC_HWND.store(hwnd.0 as usize, std::sync::atomic::Ordering::Release);
        // Lo que llegase antes de registrar la ventana.
        wake_for_ipc();
        crate::shared_theme::watch(wake_for_shared_theme);
```
c) En `wndproc`, sustituye:
```rust
            WM_SYNTAX_READY => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
```
por:
```rust
            WM_SYNTAX_READY => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_SHARED_THEME => {
                // `appearance.toml` cambió: el fundido de aquí sale solo en `WM_PAINT`
                // (`last_dark`/`last_accent`); Ajustes, si está abierto, lo arranca aparte.
                if let Some(w) = ptr.as_mut() {
                    let before = w.render_ui();
                    if crate::shared_theme::reload() {
                        let system_dark = system_uses_dark_mode();
                        apply_dark_mode(hwnd, crate::is_dark(w.render_ui().theme, system_dark));
                        if let Some(raw) = OPEN_SETTINGS_HWND.with(|c| c.get()) {
                            crate::settings_window::shared_theme_changed(HWND(raw as *mut _), crate::is_dark(before.theme, system_dark), before.accent);
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
```

- [ ] **Step 4: `settings_window.rs`: fundido al cambiar desde fuera**

En `E:\notty\crates\notty-ui\src\settings_window.rs`, sustituye:
```rust
fn slider_to(st: &mut State, x: f32) {
```
por:
```rust
/// El tema o el acento efectivos cambiaron desde fuera (`appearance.toml` de essentials):
/// el mismo fundido de 350 ms que al elegirlos aquí, y la fila de Apariencia al día.
pub fn shared_theme_changed(hwnd: HWND, was_dark: bool, was_accent: notty_config::AccentColor) {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        let Some(st) = ptr.as_mut() else { return };
        start_theme_fade(st, hwnd, was_dark, was_accent);
        ensure_anim_timer(st, hwnd);
        invalidate(hwnd);
    }
}

fn slider_to(st: &mut State, x: f32) {
```

- [ ] **Step 5: Compilar y tests existentes**

Run:
```powershell
Set-Location E:\notty
cargo build --release -p notty
cargo test -p notty-ui
```
Expected: `Finished`; tests `ok`. Si `ReadDirectoryChangesW` o `CreateFileW` piden otro tipo (p. ej. `BOOL` en vez de `bool`, o falta una feature), sigue al compilador sin cambiar el comportamiento.

- [ ] **Step 6: Prueba manual en caliente**

Run (en pwsh, desde `E:\notty`):
```powershell
$v = (Select-String -Path E:\notty\crates\notty\Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$cfg = "E:\notty\.cache\appdata\Roaming\notty\config.toml"
$ess = "E:\notty\.cache\appdata\Roaming\essentials"
$app = "$ess\appearance.toml"
New-Item -ItemType Directory -Force $ess | Out-Null
Set-Content -LiteralPath $cfg -Encoding utf8NoBOM -Value @"
first_run_done = true
last_seen_version = "$v"

[ui]
theme = "light"
accent = "rojo"
"@
function Write-Appearance([string]$theme, [string]$accent) {
  # Como essentials: temporal y rename en la misma carpeta.
  Set-Content -LiteralPath "$ess\appearance.toml.tmp" -Encoding utf8NoBOM -Value "theme = `"$theme`"`naccent = `"$accent`"`nshared = true"
  Move-Item -LiteralPath "$ess\appearance.toml.tmp" -Destination $app -Force
}
Write-Appearance 'oscuro' 'morado'
$id = & E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t5-a-main.png -Main -KeepOpen
Write-Appearance 'claro' 'naranja'
Start-Sleep -Milliseconds 800
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t5-b-main.png -ProcId $id -Main -KeepOpen | Out-Null
# Con Ajustes abierto: también cambia en caliente.
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t5-c-ajustes.png -ProcId $id -KeepOpen | Out-Null
Write-Appearance 'oscuro' 'verde'
Start-Sleep -Milliseconds 800
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t5-d-ajustes.png -ProcId $id -KeepOpen | Out-Null
# shared = false: vuelve a los suyos (claro + rojo).
Set-Content -LiteralPath $app -Encoding utf8NoBOM -Value "theme = `"oscuro`"`naccent = `"verde`"`nshared = false"
Start-Sleep -Milliseconds 800
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t5-e-ajustes.png -ProcId $id
@(Get-Process notty -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq "E:\notty\target\release\notty.exe" }).Count
# Sin la carpeta de essentials al arrancar: arranca normal (sin vigilante).
Remove-Item -LiteralPath "E:\notty\.cache\appdata\Roaming\essentials\appearance.toml"
Remove-Item -LiteralPath "E:\notty\.cache\appdata\Roaming\essentials"
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t5-f-sin-carpeta.png -Main
```
Expected: `t5-a-main.png` oscuro/morado; `t5-b-main.png` claro/naranja **sin reiniciar** notty; `t5-c-ajustes.png` Ajustes en claro/naranja con Tema «Claro» y «Naranja» marcados; `t5-d-ajustes.png` oscuro/verde, ventana principal detrás también; `t5-e-ajustes.png` claro/rojo (los propios); `0` notty de pruebas abiertos al final; `t5-f-sin-carpeta.png` claro/rojo. El fundido en sí (350 ms, el de `window.rs:2112-2149` y `settings_window.rs:1141-1156`) no se ve en una imagen fija: es el mismo código que ya se usa al elegir tema, y las capturas esperan 900 ms a que acabe.

- [ ] **Step 7: Commit**

```powershell
git -C E:\notty status --short
git -C E:\notty add crates/notty-ui/Cargo.toml crates/notty-ui/src/shared_theme.rs crates/notty-ui/src/window.rs crates/notty-ui/src/settings_window.rs
git -C E:\notty commit -m "feat(tema): el tema de essentials cambia en caliente (ReadDirectoryChangesW, fundido de 350 ms)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
(Si `git status` enseña `Cargo.lock` cambiado, añádelo también al `git add`.)

---

### Task 6: Versión 1.1.0, CHANGELOG y Novedades

**Files:**
- Modify: `crates/notty/Cargo.toml:3`, `crates/notty-setup/Cargo.toml:3` (versión; `release.ps1` exige que coincidan; `notty-setup` enseña la suya con `ui::VERSION = env!("CARGO_PKG_VERSION")`, `crates/notty-setup/src/ui.rs:17`)
- Modify: `Cargo.lock` (lo actualiza `cargo`)
- Modify: `CHANGELOG.md:3-13`
- Modify: `crates/notty-ui/src/whats_new.rs:35-60`
- Modify: `crates/notty-ui/src/strings.rs`

- [ ] **Step 1: Versión**

En `E:\notty\crates\notty\Cargo.toml` y en `E:\notty\crates\notty-setup\Cargo.toml` sustituye `version = "1.0.0"` por:
```toml
version = "1.1.0"
```

- [ ] **Step 2: CHANGELOG**

En `E:\notty\CHANGELOG.md` sustituye (líneas 3-13, la sección «Sin publicar» de `9aeebdc`):
```markdown
## Sin publicar

- El aviso de versión nueva y la página Ajustes → Actualizaciones salen completamente en inglés con la app en inglés: título, botones, errores y también las notas de la versión.
- Las notas de cada versión pueden llevar su versión en inglés debajo de un encabezado `#### English`.

#### English

- The new-version popup and Settings → Updates are now fully in English when the app is: title, buttons, errors and the release notes too.
- Each version's notes can carry an English version under an `#### English` heading.
```
por:
```markdown
## v1.1.0

- notty se puede dejar en manos de essentials: si la tienda lo gestiona (`[updates] managed_by = "essentials"` en `config.toml`, lo pone essentials), notty no busca ni descarga versiones nuevas y Ajustes → Actualizaciones lo dice, con «Abrir en essentials». Novedades sigue en Ajustes → Acerca de.
- Tema compartido con essentials: si existe `%APPDATA%\essentials\appearance.toml` con `shared = true`, notty usa su tema y su color de acento y los cambia en caliente, con el mismo fundido de siempre. Ajustes → Apariencia → «Seguir el tema de essentials» lo apaga; encendido, elegir tema o acento en notty también lo cambia en essentials.
- Cada release publica también `notty.msi` con su firma `notty.msi.sig`, y el MSI registra la carpeta de instalación para que essentials la encuentre.
- El aviso de versión nueva y la página Ajustes → Actualizaciones salen completamente en inglés con la app en inglés: título, botones, errores y también las notas de la versión.
- Las notas de cada versión pueden llevar su versión en inglés debajo de un encabezado `#### English`.

#### English

- notty can be managed by essentials: when the store manages it (`[updates] managed_by = "essentials"` in `config.toml`), notty doesn't check for or download new versions and Settings → Updates says so, with "Open in essentials". What's new is still in Settings → About.
- Theme shared with essentials: with `%APPDATA%\essentials\appearance.toml` and `shared = true`, notty uses its theme and accent color and follows changes live. Settings → Appearance → "Follow the essentials theme" turns it off; while on, picking a theme or accent in notty changes it in essentials too.
- Every release also ships `notty.msi` with its `notty.msi.sig` signature, and the MSI records the install folder so essentials can find it.
- The new-version popup and Settings → Updates are now fully in English when the app is: title, buttons, errors and the release notes too.
- Each version's notes can carry an English version under an `#### English` heading.
```

- [ ] **Step 3: Novedades**

En `E:\notty\crates\notty-ui\src\whats_new.rs` sustituye el bloque entero de `RELEASES` (líneas 35-60):
```rust
/// La más nueva primero.
pub const RELEASES: &[Release] = &[Release {
    version: "1.0.0",
```
…hasta el `}];` final de ese bloque, por:
```rust
/// La más nueva primero.
pub const RELEASES: &[Release] = &[
    Release {
        version: "1.1.0",
        items: &[
            Item {
                icon: Icon::Sparkle,
                title: "Actualizaciones desde essentials",
                desc: "Si tienes essentials, notty se actualiza desde la tienda. Lo verás en Ajustes → Actualizaciones.",
            },
            Item {
                icon: Icon::Sparkle,
                title: "El mismo tema que essentials",
                desc: "notty sigue el tema y el color de acento de essentials al momento. Se apaga en Ajustes → Apariencia.",
            },
        ],
    },
    Release {
        version: "1.0.0",
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
            Item {
                icon: Icon::Vim,
                title: "Más comandos de vim",
                desc: ":q!, :qa, :wa, :e archivo, :e!, :tabnew, :bn/:bp, :12 para ir a una línea y más.",
            },
        ],
    },
];
```

- [ ] **Step 4: Traducciones de Novedades**

En `E:\notty\crates\notty-ui\src\strings.rs` sustituye:
```rust
    (":q!, :qa, :wa, :e archivo, :e!, :tabnew, :bn/:bp, :12 para ir a una línea y más.", ":q!, :qa, :wa, :e file, :e!, :tabnew, :bn/:bp, :12 to jump to a line and more."),
```
por:
```rust
    (":q!, :qa, :wa, :e archivo, :e!, :tabnew, :bn/:bp, :12 para ir a una línea y más.", ":q!, :qa, :wa, :e file, :e!, :tabnew, :bn/:bp, :12 to jump to a line and more."),
    ("Actualizaciones desde essentials", "Updates from essentials"),
    ("Si tienes essentials, notty se actualiza desde la tienda. Lo verás en Ajustes → Actualizaciones.", "If you have essentials, notty updates from the store. You'll see it in Settings → Updates."),
    ("El mismo tema que essentials", "The same theme as essentials"),
    ("notty sigue el tema y el color de acento de essentials al momento. Se apaga en Ajustes → Apariencia.", "notty follows the essentials theme and accent color instantly. Turn it off in Settings → Appearance."),
```

- [ ] **Step 5: Compilar, tests y ver Novedades**

Run (en pwsh, desde `E:\notty`):
```powershell
cargo build --release -p notty -p notty-setup
cargo test -p notty-ui
Set-Content -LiteralPath "E:\notty\.cache\appdata\Roaming\notty\config.toml" -Encoding utf8NoBOM -Value @"
first_run_done = true
last_seen_version = "1.0.0"

[ui]
theme = "dark"
"@
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t6-novedades.png -Main
& E:\notty\tools\shot-settings.ps1 -Out E:\notty\.cache\shots\t6-acerca.png -Steps 'click:26,288'
Select-String -LiteralPath "E:\notty\.cache\appdata\Roaming\notty\config.toml" -Pattern 'last_seen_version'
```
Expected: `Finished`; tests `ok` (los de `whats_new` usan `RELEASES[0]`, ahora 1.1.0); `t6-novedades.png`: el popup de Novedades de 1.1.0 con las dos entradas y su destello, como el de 1.0.0; `t6-acerca.png`: chip `v1.1.0`; `last_seen_version = "1.1.0"`.

- [ ] **Step 6: Commit**

```powershell
git -C E:\notty add crates/notty/Cargo.toml crates/notty-setup/Cargo.toml Cargo.lock CHANGELOG.md crates/notty-ui/src/whats_new.rs crates/notty-ui/src/strings.rs
git -C E:\notty commit -m "chore: version 1.1.0" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Repaso contra el spec y limpieza

**Files:**
- Modify: solo lo que salga del repaso.

- [ ] **Step 1: Release 1.1.0 sin publicar, firmas y tabla**

Run (con las variables de entorno de Global Constraints y RAM ≥ 4 GB):
```powershell
Set-Location E:\notty
pwsh -NoProfile -File tools\release.ps1 -Tag v1.1.0 -NoPublish
$sign = "E:\notty\target\release\notty-sign.exe"
foreach ($f in "E:\notty\target\release\notty.msi", "E:\notty\target\release\notty-setup.exe") { & $sign --verify $f --sig "$f.sig"; "$f → $LASTEXITCODE" }
```
Expected: `Listo sin publicar: …`; dos `firma válida` / `→ 0`. Repite el snippet COM de la Task 1, Step 7: misma fila `… | ARPINSTALLLOCATION | [INSTALLFOLDER]` y secuencia > 1000. Lee además la versión (solo lectura) con el mismo `Get-MsiRows`: `Get-MsiRows $db "SELECT Value FROM Property WHERE Property='ProductVersion'"` → `1.1.0`.

- [ ] **Step 2: Recorrido del spec**

Para cada punto, apunta cómo se comprobó (paso y captura o salida). Lo que no se cumpla, arréglalo aquí y vuelve a comprobarlo:
- 1 MSI firmado como asset: `notty.msi` + `notty.msi.sig` (64 bytes, ed25519 crudo, misma clave que `notty-setup.exe`) en `target\release\`, comprobados con `--verify` (válida 0 / NO válida 1) y subidos en `gh release create` cuando no hay `-NoPublish` (leer `tools/release.ps1`) — Task 1 Step 6, Task 7 Step 1.
- 2 Carpeta registrada: `SetProperty ARPINSTALLLOCATION=[INSTALLFOLDER]` tras `CostFinalize` en la tabla del MSI — Task 1 Step 7.
- 3 `managed_by`: sin comprobación al arrancar (`last_check = 0`) ni manual ni descarga (leer `check_updates_now`/`start_update_download`); solo la tarjeta «Gestionado por essentials · Las actualizaciones de notty se instalan desde la tienda» con «Abrir en essentials» (`ShellExecuteExW` `essentials.exe --app notty`, `SEE_MASK_FLAG_NO_UI`); fallo → «essentials no está instalado» en `danger` + «Volver a gestionar yo las actualizaciones», que borra la clave; Novedades sigue — Task 3 Step 9.
- 3 (riesgo del contrato): un guardado de notty no borra el `managed_by` que essentials escribió con notty abierto — Task 3 Step 9.6.
- 4 Tema compartido: usa `theme`/`accent` con `shared = true` y `follow_essentials` (por defecto sí); vigila la carpeta con `ReadDirectoryChangesW` y aplica con el fundido de 350 ms (ventana y Ajustes); fila «Seguir el tema de essentials» solo si el archivo existe; encendida, elegir tema/acento escribe `appearance.toml` (temporal + rename, conserva `shared`); apagada, usa y guarda el suyo; «sistema» → Azul — Task 4 Step 10, Task 5 Step 6.
- 6 Versión 1.1.0 en `crates/notty` y `crates/notty-setup`, CHANGELOG y Novedades — Task 6.
- Compatibilidad: sin `managed_by` ni `appearance.toml`, notty 1.1.0 se comporta como hoy — Task 2 Step 5 (referencia), Task 3 Step 9.5, Task 4 Step 10.5, Task 5 Step 6 (sin carpeta).

- [ ] **Step 3: Tests y limpieza del equipo**

Run:
```powershell
Set-Location E:\notty
cargo test -p notty-config -p notty-update -p notty-ui -p notty
@(Get-Process notty -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq "E:\notty\target\release\notty.exe" }) | Select-Object Id, Path
(Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).notty
git -C E:\notty status --short
git -C E:\notty log --oneline -8
```
Expected: tests `ok`; ningún notty de `target\release` abierto (si queda alguno tuyo, `Stop-Process -Id <PID>`); el valor `Run\notty` es el mismo que antes de empezar (el del notty instalado o nada; nunca `E:\notty\target\…`); `git status` limpio (`.cache/`, `target/` e `installer/notty.msi` ignorados); los commits de las Tasks 1-6 encima de `9aeebdc`. `E:\notty\.cache\` se queda (perfil de pruebas y capturas, ignorado).

- [ ] **Step 4: Commit**

```powershell
git -C E:\notty commit --allow-empty -m "chore: repaso de notty 1.1 gestionado por essentials contra el spec" -m "<lista del Step 2 con cómo se comprobó cada punto>" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
(Si el repaso cambió archivos, añádelos antes con `git -C E:\notty add <rutas explícitas>` y quita `--allow-empty`.)

## Lo que prueba el usuario (fuera de este plan)

- Publicar la release v1.1.0 (`pwsh tools/release.ps1 -Tag v1.1.0 -NotesFile <notas>` sin `-NoPublish`, con el árbol limpio) y que salgan los cuatro assets: `notty-setup.exe(.sig)` y `notty.msi(.sig)`.
- Instalar el `notty.msi` 1.1.0 real (UAC perMachine) y que `MsiGetProductInfoW(INSTALLLOCATION)` devuelva `C:\Program Files\notty\` (essentials lo enseña en su página de notty).
- La migración de essentials sobre su `%APPDATA%\notty\config.toml` real (escribe `managed_by`, conserva lo demás) y que notty 1.1.0 lo respete al abrirse; con notty abierto durante la migración, comprobar que al abrir Ajustes ya sale la tarjeta.
- «Abrir en essentials» con essentials instalado (App Paths escrito por la tienda): abre la tienda en la página de notty.
- Cambiar el tema o el acento desde essentials y verlo cambiar en notty en caliente; y al revés, cambiarlo en notty y verlo en essentials.
- Un notty anterior a 1.1.0 ignora `managed_by` (serde ignora claves desconocidas) y la tienda lo actualiza.
