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
