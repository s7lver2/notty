# Construye, firma y publica una release de notty: notty.msi (instalador WiX,
# plan del instalador) + notty-setup.exe firmado con notty-sign, subidos a
# GitHub Releases con `gh`. Uso: pwsh tools/release.ps1 -Tag v1.3.0 -NotesFile CHANGELOG.md
#
# Prerequisito de una sola vez, manual (ver Task 1 del plan del actualizador,
# docs/superpowers/plans/2026-09-26-actualizador.md): crear el repo de GitHub y
# el remoto `origin` con `gh repo create <owner>/notty ...`. Este script no lo
# hace por ti — requiere decidir visibilidad y tener `gh auth login` hecho.
#
# PLACEHOLDER: $Repo de abajo (y la constante REPO de crates/notty/src/main.rs)
# deben sustituirse por el owner/repo real antes de la primera release de verdad.
param(
    [Parameter(Mandatory=$true)][string]$Tag,   # e.g. v1.3.0
    [string]$NotesFile
)

$ErrorActionPreference = "Stop"

# PLACEHOLDER: reemplázalo por el owner/repo real (ver constante REPO en
# crates/notty/src/main.rs, que debe cambiarse a la vez que esto).
$Repo = "OWNER/notty"

# Compara un tag ("v1.3.0") con la versión de Cargo.toml ("1.3.0"), sin el
# prefijo "v". Extraída a función aparte (en vez de una comparación inline en
# el cuerpo del script) para que se pueda probar sola — ver Step 3 más abajo.
function Test-TagMatchesVersion {
    param([string]$Tag, [string]$CargoVersion)
    return ($Tag -replace '^v', '') -eq $CargoVersion
}

# 1. Árbol de trabajo limpio + el tag coincide con crates/notty/Cargo.toml.
$status = git status --porcelain
if ($status) { throw "El árbol de trabajo no está limpio. Confirma o descarta los cambios primero." }

$version = ($Tag -replace '^v', '')
$cargoVersion = (Select-String -Path "crates\notty\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if (-not (Test-TagMatchesVersion -Tag $Tag -CargoVersion $cargoVersion)) {
    throw "El tag $Tag no coincide con la versión de crates/notty/Cargo.toml ($cargoVersion)."
}

# 2. Build: notty/notty-legacy, el MSI (WiX, plan del instalador) y notty-setup.
cargo build --release -p notty -p notty-legacy
if ($LASTEXITCODE -ne 0) { throw "Fallo compilando notty/notty-legacy" }

wix build installer\notty.wxs `
    -d NottyExePath=target\release\notty.exe `
    -d NottyLegacyExePath=target\release\notepad_legacy.exe `
    -o installer\notty.msi
if ($LASTEXITCODE -ne 0) { throw "Fallo construyendo el MSI" }

cargo build --release -p notty-setup
if ($LASTEXITCODE -ne 0) { throw "Fallo compilando notty-setup" }

$setupExe = "target\release\notty-setup.exe"

# 3. Firma: notty-sign lee la clave privada de %USERPROFILE%\.notty-release\ed25519.key
# por defecto (generada una vez con `notty-sign --keygen`, ver Task 5 del plan).
cargo run --release -p notty-update --features sign --bin notty-sign -- `
    --sign $setupExe --out "$setupExe.sig"
if ($LASTEXITCODE -ne 0) { throw "Fallo firmando notty-setup.exe" }

# 4. Publica en GitHub Releases.
if (-not $NotesFile) { throw "Pasa -NotesFile con las notas de la versión." }
gh release create $Tag $setupExe "$setupExe.sig" --repo $Repo --notes-file $NotesFile
if ($LASTEXITCODE -ne 0) { throw "Fallo publicando la release en GitHub" }

Write-Host "Release $Tag publicada." -ForegroundColor Green
