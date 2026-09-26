# Changelog

## v0.1.0

Primera release pública de notty.

- Instalador MSI (WiX) con UI propia en Direct2D (notty-setup), UAC, upgrade y desinstalación.
- Actualizador integrado: comprueba GitHub Releases, verifica la firma Ed25519 de `notty-setup.exe` y lo lanza.
- Tutorial guiado (spotlight tour) sobre la UI real, con teclado interactivo.
- Editor: pestañas con animación, menús contextuales, modo vim (`i` para Insert, conteos, registros), path prompt con sugerencias por frecuencia de uso y undo/portapapeles.
- Ajustes: secciones con transición cruzada, remapeo de atajos en vivo, indicadores de estado para las opciones.
- Tema claro/oscuro con crossfade en toda la app.
