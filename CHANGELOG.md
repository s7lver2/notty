# Changelog

## Sin publicar

- El aviso de versión nueva y la página Ajustes → Actualizaciones salen completamente en inglés con la app en inglés: título, botones, errores y también las notas de la versión.
- Las notas de cada versión pueden llevar su versión en inglés debajo de un encabezado `#### English`.

#### English

- The new-version popup and Settings → Updates are now fully in English when the app is: title, buttons, errors and the release notes too.
- Each version's notes can carry an English version under an `#### English` heading.

## v1.0.0

- Previsualización de Markdown al estilo GitHub: bloques de código con resaltado, citas y avisos, tablas, listas de tareas, enlaces.
- Rendimiento (detalles y cifras en `docs/rendimiento.md`): arranca 4-8 veces más rápido (primer pintado en ~70 ms en vez de ~360 ms), abrir archivos grandes ya no se bloquea con el resaltado de sintaxis, escribir en archivos de código grandes no se traba, en reposo ya no repinta ni se despierta cada 150 ms, y un archivo de 50 MB ocupa ~50 MB menos de RAM.
- `tools/bench.ps1` y `cargo run --release -p notty-ui --example bench` para medir todo esto.
- Ajustes → Ventana → Animaciones: frecuencia de 30, 60 (por defecto) o 120 Hz, y "Animaciones reducidas" (más cortas y a 30 Hz) para equipos con pocos recursos.
- Popup de Novedades la primera vez que se abre notty tras actualizarse, con una miniatura animada por novedad; se puede volver a ver en Ayuda → Novedades o en Ajustes → Acerca de → Novedades.
- Modo vim: más comandos. `:q!` (cerrar sin guardar), `:qa`, `:qa!`, `:wa`, `:wqa`/`:xa`, `:w ruta`, `:e ruta`, `:e!` (recargar del disco), `:enew`/`:tabnew`, `:bn`/`:bp`, `:12` y `:$` (ir a línea), `:set nu`/`:set nonu`. Un comando desconocido ahora avisa en vez de no hacer nada.
- Fix: el atajo global no funcionaba. En modo "Segundo plano" nadie arrancaba `notty --daemon`: ahora lo arranca notty al abrirse o al elegir ese modo, y "Iniciar con Windows" lo añade al inicio de sesión (`HKCU...Run`). En modo "Acceso directo" los `.lnk` se crean en el menú Inicio con Ctrl+Alt+N / Ctrl+Alt+Shift+N ya asignados y avisando a Explorer, que antes no se enteraba hasta reiniciar.
- Fix: la ventana de Ajustes ya no muestra un marco gris claro a su alrededor al perder el foco.

#### English

- GitHub-style Markdown preview: highlighted code blocks, quotes and alerts, tables, task lists, links.
- Performance (details in `docs/rendimiento.md`): starts 4-8× faster, large files no longer freeze on syntax highlighting, typing in big code files stays smooth, no work while idle, and a 50 MB file uses ~50 MB less RAM.
- Settings → Window → Animations: 30, 60 (default) or 120 Hz, plus "Reduced animations" for low-end machines.
- What's new popup the first time notty opens after an update, with an animated thumbnail per item; also in Settings → About → What's new.
- Vim mode: more commands. `:q!`, `:qa`, `:qa!`, `:wa`, `:wqa`/`:xa`, `:w path`, `:e path`, `:e!`, `:enew`/`:tabnew`, `:bn`/`:bp`, `:12` and `:$`, `:set nu`/`:set nonu`. Unknown commands now show a message.
- Fix: the global hotkey didn't work, in both "Background" and "Shortcut" modes.
- Fix: the Settings window no longer shows a light grey frame when it loses focus.

## v0.5.0

- Ajustes → Ventana recuerda el ancho/alto/maximizado al cerrar notty, con un botón para restablecer el tamaño de siempre.
- Nueva opción "Ajustar texto a la ventana": las líneas largas pasan a la siguiente línea en vez de salirse por el borde, y se reajustan solas al redimensionar.
- Ajustes → Archivos: "Guardar por defecto en" (Carpeta de usuario, Escritorio, Documentos, Descargas) para Guardar como.

## v0.4.0

- Sistema de idiomas: Auto (sigue el de Windows), Español o Inglés, elegible desde Ajustes → Ayuda. Traducción casi completa de la app: editor, Ajustes, menús, tour, ventana de bienvenida, panel de actualizaciones y el instalador entero (asistente, opciones, progreso de MSI, errores, pantalla de auto-actualización).
- Color de acento personalizable desde Ajustes (8 opciones), con animación al cambiarlo igual que al cambiar de tema.
- Nueva opción para reabrir los archivos que quedaron abiertos al arrancar notty.
- Nueva opción para mostrar iconos en las pestañas.
- Fix: autocompletar con Tab en Abrir/Guardar como funcionaba solo tras escribir la primera letra; ahora sugiere desde una carpeta vacía y parte de la carpeta de usuario en vez de C:\.

## v0.3.0

- Modo Paneles como kitty: pestañas que se dividen en hasta 4 paneles, con anchos ajustables.
- Modo Acomodar (Ctrl+Shift+R) para cambiar los anchos de los paneles con el teclado.
- Atajos para moverte: pestañas (Ctrl+PgUp/PgDn, Ctrl+1…9), paneles (Alt+1…9), mover pestaña o panel (Ctrl+Shift+PgUp/PgDn, Ctrl+Alt+←/→).
- Rediseño completo de Ajustes: barra lateral de iconos que se expande al pasar el cursor, subpáginas con vistas previas en vivo (Fuentes, Ligaduras, Sintaxis), animaciones y sin el borde raro.
- Sección de Actualizaciones mejorada; el aviso de versión nueva ya no sale desalineado.
- Subpágina Sintaxis: activa o desactiva cada lenguaje por separado. 11 lenguajes nuevos (Go, Java, C#, HTML, CSS, Bash, Lua, Ruby, PHP, XML, SQL).
- Logo nuevo y Acerca de rediseñado.
- Aviso de "Ruta copiada" al copiar la ruta de una pestaña.
- Fix: cerrar con cambios sin guardar es configurable (preguntar o recuperar al abrir).
- Fix: el conflicto de autoguardado se resuelve escribiendo SI o NO.
- Fix: si Direct2D pierde el dispositivo, la ventana se recupera en vez de quedarse en negro.
- Fix: abrir con notty ya abierto resuelve rutas relativas, crea archivos nuevos y no duplica ventanas; nueva opción para abrir siempre en una ventana nueva.
- Fix: el cursor ya no se queda entre \r y \n; los emoji y demás caracteres fuera del BMP ya se pueden escribir; los archivos ANSI se abren como texto.
- Fix: Guardar como pregunta antes de sobrescribir; los errores al guardar se avisan; un config.toml roto ya no se sobrescribe sin dejar copia.
- Fix: relanzar tras actualizar guarda lo no guardado y cierra también desde Ajustes.

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
