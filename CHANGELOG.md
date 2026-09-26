# Changelog

## v0.2.0

- El instalador se autoactualiza a la versión más reciente al abrirse.
- Fix: sustituir el Bloc de notas ahora funciona también en Windows 11 (Win+R → notepad abría el legacy).
- Fix: el scroll ya funciona en la vista raw/hex.
- Fix: contorno claro alrededor de Ajustes al cambiar de tema.
- Ajustes ya no permite abrir varias ventanas a la vez; si ya hay una, le hace focus.
- Barra de scroll para el editor (texto y raw).
- Iconos de carpeta/archivo activables en las sugerencias de ruta.
- Resaltado de sintaxis (tree-sitter: Rust, JS/TS, Python, C/C++, JSON, TOML, Markdown, YAML), activable en Ajustes.
- Selector nativo de Windows como opción para Abrir/Guardar como.
- Zoom de texto con Ctrl+=/Ctrl+-/Ctrl+0.
- Atajos globales consolidados en Ajustes → Teclado.
- Ligaduras visuales configurables (`->` → flecha, etc.), con set propio personalizable.
- Fuente del editor personalizable desde Ajustes.
- Modo Paneles: hasta 3 documentos lado a lado, estilo kitty, con atajos de teclado.
- Iconos de archivo personalizados por tipo (texto/config/markdown/código) al asociar notty como app por defecto.

## v0.1.0

Primera release pública de notty.

- Instalador MSI (WiX) con UI propia en Direct2D (notty-setup), UAC, upgrade y desinstalación.
- Actualizador integrado: comprueba GitHub Releases, verifica la firma Ed25519 de `notty-setup.exe` y lo lanza.
- Tutorial guiado (spotlight tour) sobre la UI real, con teclado interactivo.
- Editor: pestañas con animación, menús contextuales, modo vim (`i` para Insert, conteos, registros), path prompt con sugerencias por frecuencia de uso y undo/portapapeles.
- Ajustes: secciones con transición cruzada, remapeo de atajos en vivo, indicadores de estado para las opciones.
- Tema claro/oscuro con crossfade en toda la app.
