# notty — diseño (v1)

**Fecha:** 2026-09-24
**Estado:** aprobado en brainstorming, pendiente de revisión escrita

## Premisa

Reescribir el Bloc de notas de Windows para que sea de verdad útil y cómodo, sin
dejar de ser un bloc de notas. Extremadamente ligero, extremadamente robusto, y que
un usuario común de Windows lo use sin ninguna complicación. Influencia práctica de
vim/nano, pero en GUI.

**Fuera siempre:** IA, nube/sincronización, plugins, cualquier cosa que se aleje de
la premisa.

## Plataforma y stack

- **Solo Windows** (10/11).
- **Rust + Win32 puro** (`windows-rs`), ventana nativa con Mica y barra de título de
  Windows 11.
- **Editor dibujado por nosotros** con Direct2D/DirectWrite. IME y accesibilidad
  (UI Automation) implementados a mano.
- Un único `notty.exe`. Objetivos: < 3 MB en disco, ~5–10 MB de RAM, arranque en frío
  < 50 ms, abrir 100 MB de texto < 1 s.

## Arquitectura

Workspace de Rust con crates de una sola responsabilidad:

| Crate | Responsabilidad | Depende de |
|---|---|---|
| `notty-core` | Buffer (rope), cursores/selección, undo/redo agrupado, búsqueda. Sin nada de Windows. | `ropey`, `regex` |
| `notty-io` | Detección de tipo (texto/raw), codificación y fin de línea; guardado atómico; mmap para raw; vigilancia de cambios externos. | `core`, `memmap2`, `encoding_rs` |
| `notty-config` | `config.toml`: lectura/escritura, presets, validación, recarga en caliente. | `toml`, `serde` |
| `notty-input` | Teclas → comandos. Keymap normal y keymap vim. | `core`, `config` |
| `notty-ui` | Ventana Win32, Mica, render DirectWrite, pestañas/buffers, barras, línea de ruta/comandos, ventana de ajustes. | todos, `windows-rs` |
| `notty` (bin) | Entrada: modo app o `--daemon`. Instancia única vía named pipe. | todos |

**Principio central — todo es un comando.** Cada acción (`save`, `find_next`,
`open_path_prompt`, `toggle_vim`, …) es un comando con nombre. Atajos, modo vim,
menús y la línea `:` solo disparan comandos. Reasignar un atajo = una línea del `.toml`.

**Instancia única:** abrir un archivo con notty ya abierto (o con el daemon activo)
lo envía por el pipe a la instancia existente, que lo abre como pestaña/buffer o
ventana nueva según configuración.

## Interfaz

### Piezas configurables

La ventana se compone de piezas independientes, cada una configurable:

| Pieza | Opciones |
|---|---|
| Pestañas / buffers | en la barra de título · bajo el menú · ocultas · solo si hay > 1 archivo |
| Barra de menús | visible · oculta · aparece con `Alt` |
| Barra de atajos (estilo nano) | visible · oculta · atajos mostrados personalizables |
| Barra de estado | visible · oculta · campos mostrados (línea/col, codificación, CRLF/LF, modo…) |
| Línea de comandos | separada · fusionada con la barra de estado |
| Números de línea | sí / no |

Posiciones fijas; no hay layout libre por arrastre.

### Presets

- **Moderna** (por defecto): pestañas en la barra de título, sin menús, barra de
  atajos nano + barra de estado.
- **Clásica:** barra de menús Archivo/Editar/Buscar/Ver/Ayuda, pestañas debajo,
  barra de estado.
- **Zen:** solo texto + una línea inferior que es estado y línea de comandos a la vez;
  pestañas solo si hay más de un archivo.

Un preset es un bloque de configuración; se pueden crear y compartir presets propios.

### Multi-archivo

**Pestañas o buffers** (estilo vim, `Ctrl+Tab` / `:b`), elegible en configuración.
Ambos totalmente personalizables.

## Interacción

### Modo normal (por defecto)

Sin modos. Atajos estándar de Windows (`Ctrl+S`, `Ctrl+Z`, `Ctrl+C/V`, `Ctrl+F`…).
Barra de atajos estilo nano con los atajos más útiles.

### Modo vim (opcional)

Activable globalmente en configuración o alternable en la ventana actual con un atajo
(por defecto `Ctrl+Alt+V`). Modal real (normal/insert/visual, `hjkl`, `dd`, `/`, `:`…).
La barra de estado muestra `-- NORMAL --` / `-- INSERT --`. Es otro keymap sobre los
mismos comandos.

### Búsqueda y reemplazo

- `Ctrl+F`: línea mínima abajo (reutiliza la línea de comandos), resaltado en vivo.
  `Enter` / `Shift+Enter` siguiente/anterior, `Esc` cierra.
- `Ctrl+H`: añade campo de reemplazo. `Enter` reemplaza una, `Ctrl+Alt+Enter` todas.
- Opciones dentro de la búsqueda: `Alt+C` mayúsculas, `Alt+W` palabra completa,
  `Alt+R` regex; se muestran como indicadores pequeños.
- `F3` / `Shift+F3`: siguiente/anterior sin abrir la barra.
- Modo vim: `/`, `?`, `n`, `N`, `:%s/a/b/g`.

### Línea de ruta

Una única línea, en la propia barra de estado, para crear/abrir/guardar como.
Estética mínima: sin paneles, sin listas, sin mensajes.

- Sugerencia de la mejor coincidencia en texto gris; `Tab` acepta, `Tab` otra vez
  pasa a la siguiente.
- Correcciones al vuelo y en silencio: `/`→`\`, barras dobles, espacios sobrantes,
  mayúsculas de carpetas existentes.
- Entiende `~` (carpeta de usuario), variables de entorno (`%APPDATA%`…), `.` (carpeta
  del archivo actual), y búsqueda difusa por segmentos (`doc proy` → `Documentos\proyectos`).
- Sin extensión → `.txt` (configurable), mostrado en gris.
- Única señal extra: una palabra discreta a la derecha — *nuevo*, *existe*,
  *carpeta nueva* — y texto en rojo si hay caracteres no válidos (`: * ? " < > |`).
- Si faltan carpetas en la ruta, se **crean automáticamente** al confirmar.
- Si el archivo existe, `Enter` lo abre.
- `Ctrl+O` dentro de la línea abre el diálogo clásico de Windows.
- `Enter` confirma y la línea vuelve a ser la barra de estado normal.

## Archivos

### Temporales vs permanentes

El tipo lo decide la combinación de teclas usada para abrir notty. Por defecto
(configurables):

| Acción | Global (sistema) | Dentro de notty |
|---|---|---|
| Nuevo temporal | `Win+Alt+N` | `Ctrl+Shift+N` |
| Nuevo permanente | `Win+Alt+Shift+N` | `Ctrl+N` |

Con el mecanismo `.lnk` los globales pasan a `Ctrl+Alt+N` (temporal) y `Ctrl+Alt+M`
(permanente), por la limitación de Windows a `Ctrl+Alt+Letra`.

**Temporal** — modo configurable:
- **Borrador:** se guarda solo en `%LOCALAPPDATA%\notty\drafts\`; si nunca recibe ruta
  (`Ctrl+S`), se borra al cerrar.
- **Volátil:** solo en memoria; al cerrar desaparece sin preguntar.

**Permanente:**
- Nace con la ruta **`CLICKME`** en la barra de estado.
- Clic en `CLICKME`, o intentar guardar con `CLICKME` puesto, activa la línea de ruta
  pidiendo que escribas la ruta.
- **Autoguardado configurable**, solo activo cuando la ruta ya no es `CLICKME`
  (tras ~1 s sin teclear, mismo mecanismo atómico).

### Atajo global

Configurable entre:
- **Daemon residente:** el mismo `notty.exe --daemon`, en la bandeja (o sin icono),
  arranca con Windows, ~1–2 MB de RAM, cualquier combinación (p. ej. `Win+Alt+N`),
  apertura instantánea.
- **Acceso directo `.lnk`** en el menú Inicio con tecla asignada: sin proceso
  residente, solo `Ctrl+Alt+Letra`, algo más lento.

### Tipos de archivo

| Tipo | Detección | Comportamiento |
|---|---|---|
| **Texto** | BOM, o UTF-8/UTF-16 válido sin bytes nulos en los primeros ~8 KB | Edición normal. Conserva codificación, BOM y CRLF/LF al guardar salvo cambio explícito desde la barra de estado. |
| **Raw** | Todo lo demás (en v1, incluye lo que será "otro") | Editor hexadecimal (offset │ hex │ ASCII) sobre mmap. |
| **Otro** | — | **Fase posterior** (PDF, ofimática…). |

**Raw:**
- Siempre arranca en **solo lectura**.
- Si el archivo no es escribible (permisos, bloqueado), se queda así y el lápiz 🖉
  está deshabilitado.
- Si es escribible, el lápiz 🖉 pasa a modo escritura (edición byte a byte).

**Forzar texto → raw** con un atajo (por defecto `Ctrl+Shift+H`): configurable.

## Flujo de datos

1. **Abrir:** llega una ruta (línea `Ctrl+O`, arrastrar, "Abrir con…", pipe) →
   `notty-io` detecta tipo → texto al rope / raw a mmap → se comprueban permisos.
2. **Editar:** tecla → `notty-input` → comando → `notty-core` modifica el buffer →
   `notty-ui` repinta solo las líneas visibles.
3. **Guardar:** siempre atómico: escribir `archivo.tmp~` → flush → renombrar sobre el
   original. Si la ruta es `CLICKME`, activa la línea de ruta en su lugar.

### Cambios externos

notty vigila el archivo en disco.
- **Sin cambios locales:** recarga en silencio.
- **Con cambios locales:** no interrumpe mientras editas. **Al guardar**, avisa del
  conflicto y pregunta cuál conservar: **el mío** o **el del disco**.
  El autoguardado se pausa mientras haya conflicto.
- **Merge:** fase posterior; en v1 la opción no aparece.

## Errores y robustez

Regla: nunca perder datos, nunca colgarse.

- **Pánicos:** antes de morir se vuelcan todos los buffers sin guardar (incluidos
  volátiles, solo en este caso) a `%LOCALAPPDATA%\notty\recovery\`. Al siguiente
  arranque se ofrece recuperarlos; los volátiles se borran tras recuperarlos.
- **E/S** (disco lleno, permisos, ruta inválida, bloqueo): nunca diálogo modal; mensaje
  breve en rojo en la barra de estado; el buffer sigue intacto en memoria.
- **Archivos enormes:** por encima de un umbral configurable (por defecto 50 MB) el
  texto se abre sin resaltado de búsqueda en vivo. Raw sin límite práctico (mmap).
- **Codificación dudosa:** nunca se "arregla" en silencio. Se abre en solo lectura con
  aviso y se ofrece reabrir en raw o elegir codificación.
- **`config.toml` roto:** se arranca con la configuración por defecto, se marca la línea
  con error y no se sobrescribe el archivo del usuario.
- **Daemon caído:** la app funciona igual; el atajo global deja de responder hasta el
  siguiente arranque. Si la combinación ya la usa otro programa, se avisa en ajustes.

## Configuración

- **Fuente única de verdad:** `config.toml` en `%APPDATA%\notty\`.
- **Ventana de ajustes gráfica** estilo Configuración de Windows que lee y escribe ese
  mismo archivo.
- **Recarga en caliente** al guardar el `.toml` (que notty puede abrir en sí mismo).
- Todo lo configurable de este documento vive ahí: preset y piezas, pestañas/buffers,
  keymaps, modo vim global, modo temporal, autoguardado, atajo global y su mecanismo,
  forzar raw, umbral de archivo grande, extensión por defecto.

## Pruebas

- Tests unitarios (`cargo test`) en `notty-core`, `notty-io`, `notty-config` y
  `notty-input`: edición/undo, búsqueda/regex, detección de codificación con archivos
  de muestra (UTF-8/16 con/sin BOM, ANSI, binarios, vacíos), guardado atómico con
  fallos simulados, corrección/autocompletado de rutas, keymaps normal y vim.
- Fuzzing (`cargo-fuzz`) sobre detección de tipo y buffer.
- `notty-ui`: pruebas manuales guiadas y tests de humo (abrir, escribir, guardar,
  cerrar) vía UI Automation.
- Presupuesto de rendimiento vigilado en CI (tamaño de binario, arranque, apertura de
  100 MB).

## Alcance

**v1:** todo lo descrito en este documento.

**Fases posteriores:**
- Merge de conflictos con cambios externos.
- Tipo de archivo "otro" (PDF, ofimática…).
- Tutorial + configuración del primer arranque.
- Resaltado de sintaxis.

**Nunca:** IA, nube/sincronización, plugins.
