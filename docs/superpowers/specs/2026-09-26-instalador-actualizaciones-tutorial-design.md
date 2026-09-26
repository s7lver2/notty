# notty — Instalador, actualizaciones y tutorial

**Fecha:** 2026-09-26 · **Estado:** diseño aprobado, pendiente de planes

Tres sub-proyectos con un diseño común y **tres planes de implementación separados**, en este orden:

1. **Instalador**: `notepad_legacy`, MSI y `notty-setup`.
2. **Actualizador**: `notty-update`, integración en notty y `tools/release.ps1`.
3. **Tutorial**: ventana de bienvenida y recorrido.

Mockups aprobados (interactivos; abrir en navegador): `docs/mockups/setup/instalador-flujo.html`,
`docs/mockups/setup/instalador-opciones.html`, `docs/mockups/setup/tutorial.html`.

## Principios

- **Sin red sin permiso.** notty no se conecta a nada hasta que el usuario activa las actualizaciones (opt-in).
  La única conexión es la consulta de la última release: sin telemetría y sin identificadores.
- **No tocar archivos del sistema.** No se renombra ni se modifica `System32\notepad.exe`. La sustitución se hace
  solo con claves de registro, que desaparecen al desinstalar.
- **Una sola versión** en `[workspace.package] version`, que usan el MSI, el setup, el actualizador y el tag.
- **Movimiento reducido respetado.** Si `SPI_GETCLIENTAREAANIMATION` está desactivado, todas las animaciones del
  setup, del tutorial y del recorrido pasan a cortes instantáneos.

---

## 1 · Instalador

### Arquitectura

```
notty-setup.exe            bin Rust (crate notty-setup). Interfaz Direct2D que reutiliza el renderer,
 │                         el tema y las animaciones de notty-ui. Corre SIN elevar.
 ├─ notty.msi embebido     include_bytes!; se extrae a %TEMP% al instalar
 └─ msi.dll                MsiSetExternalUIRecord + MsiInstallProduct: MSI ejecuta sin interfaz
                           propia y manda progreso/ActionText/errores a nuestro callback
```

- **Instalación para todo el equipo** en `%ProgramFiles%\notty`: `notty.exe` y `notepad_legacy.exe`.
- **Elevación**: la interfaz del setup nunca se eleva. Es MSI el que pide UAC al empezar la secuencia de ejecución.
  Así el setup puede relanzar notty al terminar **como el usuario**, nunca como admin.
- **MSI** (`installer/notty.wxs`, WiX v5, solo en build): `MajorUpgrade`, entrada en "Aplicaciones instaladas",
  rollback y desinstalación. La desinstalación usa la interfaz básica estándar de MSI (no hace falta la nuestra).

### Features de MSI ↔ toggles de la interfaz

Cada toggle es una Feature y el setup traduce la selección a `ADDLOCAL=`. `Core` es obligatoria y no se muestra.

| Grupo (interfaz) | Toggle | Feature | Qué escribe | Defecto |
|---|---|---|---|---|
| Integración con Windows | Sustituir el Bloc de notas | `NotepadReplace` | `HKLM\…\Image File Execution Options\notepad.exe` → `Debugger = "<dir>\notty.exe"` | ☑ |
| | "Abrir con notty" en el menú contextual | `ContextMenu` | `HKLM\Software\Classes\*\shell\notty` | ☑ |
| | Usar notty desde la terminal | `PathEnv` | carpeta de instalación en el PATH del sistema | ☑ |
| Tipos de archivo | Texto plano (.txt .log .text) | `AssocText` | ProgID `notty.txt`, `OpenWithProgids`, `Capabilities` + `RegisteredApplications` | ☑ |
| | Configuración (.ini .cfg .conf .toml) | `AssocConfig` | igual | ☑ |
| | Markdown (.md .markdown) | `AssocMarkdown` | igual | ☑ |
| | Código y datos (.json .xml .csv .yaml) | `AssocCode` | igual | ☐ |
| Accesos directos | Menú Inicio | `StartMenu` | acceso directo | ☑ |
| | Escritorio | `Desktop` | acceso directo | ☐ |
| Avanzado | Carpeta de instalación | `INSTALLFOLDER` | propiedad, no Feature | `%ProgramFiles%\notty` |

- **El predeterminado de `.txt` no se puede fijar en silencio** (hash de `UserChoice`). La pantalla final ofrece
  "Abrir Aplicaciones predeterminadas" (`ms-settings:defaultapps`).
- **Menú contextual**: la entrada clásica aparece en "Mostrar más opciones" en Windows 11. El menú moderno necesita
  un paquete MSIX o *sparse package* y queda fuera de este alcance.
- **Opciones futuras**: una Feature nueva + una fila en la tabla de opciones del setup (y, si se quiere, una escena
  de vista previa). La ventana no cambia de tamaño.

### IFEO y `notepad_legacy`

- **Argumento de IFEO en notty**: IFEO invoca `notty.exe "<ruta>\notepad.exe" [args…]`. En el arranque, si
  `argv[1]` termina en `\notepad.exe` (sin distinguir mayúsculas), se descarta. Función pura con tests.
- **`notepad_legacy.exe`** (crate `notty-legacy`):
  1. `CreateProcessW("%SystemRoot%\System32\notepad.exe", args, DEBUG_ONLY_THIS_PROCESS)`. Windows no aplica
     IFEO a procesos creados con depuración.
  2. `DebugSetProcessKillOnExit(FALSE)` + `DebugActiveProcessStop(pid)` para soltarlo al momento.
  3. Si falla: `explorer.exe shell:AppsFolder\Microsoft.WindowsNotepad_8wekyb3d8bbwe!App` (Notepad de la Store).
     En ese caso los argumentos de archivo se pierden, y se documenta.
- Es **la única pieza no garantizada sin probar**, así que el plan 1 empieza con una prueba de concepto manual
  antes de construir nada más.

### Interfaz (ver `instalador-flujo.html` e `instalador-opciones.html`)

- Ventana fija sin redimensionar, titlebar de 32px como notty, tema claro/oscuro del sistema.
- **Carril de pasos** a la izquierda (Bienvenida · Opciones · Instalar · Listo), con una línea de progreso que se
  rellena al avanzar. En "Instalar" va sincronizada con el progreso real.
- **Opciones en grupos desplegables** al estilo de los expanders de Ajustes de Windows 11:
  - la altura se anima en 260ms y el chevron gira
  - las filas entran escalonadas (30ms entre una y otra)
  - la cabecera muestra un resumen en vivo del tipo "2 de 3"
  - los toggles son de estilo Windows 11
- **Vista previa en vivo** en un panel a la derecha que ilustra la opción bajo el ratón: una terminal escribiendo
  `notepad notas → notty`, el menú contextual, la lista de asociaciones, el menú Inicio y la carpeta. Cambia de
  escena con fundido y escala.
- **Transiciones entre pasos**: el contenido sale 12px a la izquierda con fundido (140ms) y entra desde la
  derecha (180ms, ease-out). El pie no se mueve.
- **Progreso**: barra de 3px que interpola el porcentaje de MSI sin saltos. El ActionText de MSI se traduce y
  cambia con un fundido.
- **Listo**: un ✓ que se dibuja con un trazo animado (600ms), un enlace "Abrir Aplicaciones predeterminadas" y el
  botón primario "Abrir notty".
- **Detalles de Windows**: escudo UAC en el botón que eleva; Enter pulsa el botón primario y Esc equivale a Atrás o
  Cancelar.

### Modo actualización

`notty-setup.exe --update --relaunch`: una sola pantalla con `1.2.0 → 1.3.0 · firma verificada ✓`, las notas de la
release y los botones "Más tarde" / "Actualizar" (con escudo). Al terminar se relanza notty como el usuario, y
notty restaura la sesión.

### Errores

- **Fallo de MSI**: MSI hace rollback atómico. El setup muestra el mensaje con el código de MSI y un enlace al log
  `%TEMP%\notty-install.log` (lo genera siempre, con `MsiEnableLog`).
- **UAC cancelado** (1602): se vuelve a la pantalla de Opciones, sin mostrar error.
- **Otra instalación de MSI en curso** (1618): mensaje "Espera a que termine la otra instalación" con Reintentar.

---

## 2 · Actualizador

### Configuración (`notty-config`)

```toml
[updates]
check = false        # opt-in; lo activan el tutorial (paso "Privacidad") o Ajustes → Acerca de
last_check = 0       # epoch en segundos
```

### `notty-update` (lib, sin interfaz)

- `latest_release(owner_repo) -> Result<Release>`: WinHTTP (crate `windows`) hace
  `GET https://api.github.com/repos/<owner>/notty/releases/latest` con `User-Agent: notty/<versión>` y
  `Accept: application/vnd.github+json`, y el resultado se parsea con `serde_json`. `Release` contiene: `tag`,
  `version`, `body` (notas) y las URLs de los assets `notty-setup.exe` y `notty-setup.exe.sig`.
- `is_newer(current, candidate) -> bool`: semver `MAJOR.MINOR.PATCH`. Los tags con sufijo (`-beta`…) se ignoran.
- `due(last_check, now) -> bool`: devuelve true si han pasado 24h o más.
- `download(url, dest)`: por WinHTTP, con callback de progreso.
- `verify(bytes, sig, PUBKEY) -> bool`: Ed25519 con `ed25519-dalek`, solo verificación. `PUBKEY` es una constante
  de 32 bytes compilada en el binario.

### Flujo en notty

1. Al arrancar, si `check && due(...)`, se lanza un hilo en segundo plano que consulta y actualiza `last_check`.
   Nunca bloquea la interfaz. "Buscar ahora" en Ajustes → Acerca de hace lo mismo a mano.
2. Si hay versión nueva, aparece "Actualización X.Y.Z disponible" en la barra de estado. Al hacer clic se abre un
   panel con las notas de la release.
3. "Actualizar" descarga el setup y su `.sig` a `%TEMP%\notty-update\`, con el progreso en el panel.
4. Se verifica la firma. **Si falla, se borran los archivos y se muestra un error: nunca se ejecuta.**
5. notty guarda la sesión (la recuperación que ya existe), lanza `notty-setup.exe --update --relaunch` y se cierra.

### Release (`tools/release.ps1`, local)

1. Comprueba que el árbol de trabajo está limpio y que el tag `vX.Y.Z` coincide con la versión del workspace. Si
   no, aborta.
2. `cargo build --release` (notty, notty-legacy), `wix build` → `notty.msi` y `cargo build --release -p notty-setup`
   (que embebe el MSI).
3. Firma `notty-setup.exe` con la clave privada de `%USERPROFILE%\.notty-release\ed25519.key` y genera
   `notty-setup.exe.sig`. La firma la hace un binario auxiliar `notty-sign` (`src/bin/` del crate `notty-update`, tras la
   feature `sign`, de modo que la lógica de firmar no entra en notty).
4. `gh release create vX.Y.Z notty-setup.exe notty-setup.exe.sig --notes-file <notas>`.

- La clave privada **nunca** entra en el repo. Generarla es un paso único y documentado (`notty-sign --keygen`), que escribe
  la clave pública para pegarla en `PUBKEY`.
- **Requisito previo**: el repo todavía no tiene remote. Hay que crear el repo de GitHub y fijar `owner/repo` como
  constante antes de la primera release.

### Errores

- **Sin red, rate limit (403/429), JSON inesperado o sin assets**: en la comprobación automática, fallo silencioso
  (solo se actualiza `last_check`); en la manual, un mensaje claro.
- **Descarga cortada**: se borra lo parcial y se ofrece Reintentar en el panel.
- **Firma inválida**: error "La descarga no es auténtica", se borran los archivos y no se hace nada más.

---

## 3 · Tutorial (ver `tutorial.html`)

### Ventana de bienvenida (`notty-ui/src/welcome_window.rs`)

- No modal, flotando sobre la ventana principal. Reutiliza los componentes de Ajustes (radios, segmentados) y el
  mismo sistema de movimiento que el instalador: carril de pasos y transiciones de 140/180ms.
- **Pasos**: Hola · Estilo (moderna/clásica) · Tema (sistema/claro/oscuro) · Teclado (normal/vim/nano, con chips
  de muestra como `hjkl · dd · :w`) · Privacidad (activar la comprobación de actualizaciones, apagada por
  defecto) · Listo.
- **Cada elección se aplica en vivo**: se escribe en la config y notty se repinta con su animación de tema.
- "Saltar" está siempre visible; Esc cierra.
- **Listo** ofrece "Enséñame dónde está todo →", que abre el recorrido.

### Recorrido (`notty-ui/src/tour.rs`)

- Una capa sobre la ventana principal: un velo oscuro con un **foco único** que se desliza y cambia de forma entre
  objetivos (450ms), más un globo que lo sigue con puntos de progreso y atajos en estilo `kbd`.
- Las posiciones salen del `Frame` de `layout.rs`, así que funciona en la vista moderna y en la clásica.
- **Paradas**: Pestañas (Ctrl+T/Ctrl+W) · Ajustes · Abrir rápido (Ctrl+O) · Barra de estado · Para curiosos
  (Ctrl+Alt+V vim, Ctrl+Shift+H bytes en crudo).
- Esc o un clic fuera lo cierran.

### Cuándo aparece

- `first_run_done = false` en la config: se muestra una sola vez. Saltar, Esc o Terminar lo marcan como hecho.
- Ajustes → Ayuda → "Repetir tutorial". Nunca se vuelve a mostrar solo.

---

## Pruebas

**Unitarias** (solo lógica pura):
- `strip_ifeo_arg`
- traducción de toggles a `ADDLOCAL`
- `is_newer`
- `due`
- parseo de la respuesta de GitHub: caso real, sin assets, JSON inválido
- `verify`: firma válida, bytes alterados, clave incorrecta
- validación versión/tag de `release.ps1`, extraída a función testeable si procede

**Manuales** (lista de comprobación en cada plan):
- instalación limpia con todas las opciones y con el mínimo
- `notepad`, Win+R `notepad`, `notepad archivo.txt` y `notepad_legacy`
- menú contextual y "Abrir con"
- UAC cancelado
- actualización 0.x → 0.y con sesión restaurada
- firma alterada rechazada
- desinstalación que deja el registro limpio
- movimiento reducido

## Fuera de alcance

- Menú contextual moderno de Windows 11 (MSIX / sparse package).
- Firma Authenticode: se puede añadir encima más adelante.
- Canal beta / prereleases.
- Interfaz propia para desinstalar.
- Actualización delta. Siempre se descarga el setup completo, que pesa unos 2–3 MB.
