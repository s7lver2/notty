fn main() {
    // notty.msi es producido por tools/release.ps1 (plan 2) hacia installer/notty.msi
    // antes de compilar notty-setup para una versión real. En compilaciones de
    // desarrollo donde todavía no existe, se escribe un marcador vacío para que
    // `cargo build` no falle -- ejecutar notty-setup localmente sin un MSI real
    // simplemente fallará en `MsiInstallProduct` con un error claro.
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("notty.msi");
    let src = std::path::Path::new("../../installer/notty.msi");
    if src.exists() {
        std::fs::copy(src, &dest).unwrap();
    } else if !dest.exists() {
        std::fs::write(&dest, []).unwrap();
    }
    println!("cargo:rerun-if-changed=../../installer/notty.msi");
}
