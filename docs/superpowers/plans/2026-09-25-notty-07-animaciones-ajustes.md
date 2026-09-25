# notty · Plan 7: Animaciones suaves y Ajustes pulido

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que abrir un menú, una lista de sugerencias, un desplegable de Ajustes o la propia ventana de Ajustes se sienta suave (fundido + un pequeño desplazamiento, no un "pop" instantáneo), que las pestañas activen con una transición corta en vez de cambiar de golpe, y que el interruptor (`toggle`) de Ajustes deslice de verdad en vez de saltar. Además, un repaso visual de Ajustes: la opción seleccionada de un desplegable abierto no se distingue de las demás (sin marca de check), y los controles (segmentados, toggles, fila de navegación) se pulen para acercarse más a la maqueta.

**Architecture:** Un módulo puro nuevo, `notty-ui/src/anim.rs`, con un tipo `Anim` (inicio, fin, instante de arranque, duración) y una curva de aceleración (`ease_out_cubic`), que da un valor interpolado `0.0..1.0` a partir de `Instant::now()` — sin acceso al reloj dentro del tipo, para poder testearlo con instantes fijos. `WindowState` (ventana principal) y el `State` de Ajustes guardan unas pocas animaciones con nombre (no un sistema genérico de propiedades: la lista de qué anima está cerrada a propósito, ver más abajo) en campos `Option<Anim>`. Mientras haya alguna animación en curso, un `SetTimer` corto (16 ms, ~60 Hz) repinta; en cuanto la última termina, se para el temporizador — la app no gasta un solo ciclo de más en reposo, coherente con "extremadamente ligera".

**Tech Stack:** Rust stable (MSVC), `windows` 0.62.2 (ya en el workspace: `SetTimer`/`KillTimer`/`WM_TIMER` ya se usan para el autoguardado). No se añaden dependencias.

**Qué anima y qué no (alcance cerrado a propósito):**
1. Menú de la barra de menús al abrirse/cerrarse: fundido + 4px de desplazamiento vertical, ~120ms.
2. Caja de sugerencias de la línea de ruta al aparecer: mismo tratamiento.
3. Desplegable (`Select`) de Ajustes al abrirse: mismo tratamiento.
4. Pestaña activa: el fondo funde hacia el color activo en ~100ms al cambiar de pestaña, en vez de aparecer de golpe.
5. Interruptor (`toggle`) de Ajustes: la bolita desliza de un lado a otro en ~140ms.
6. Ventana de Ajustes al abrirse: funde de 0 a 1 y escala de 97% a 100% en ~150ms.

Todo lo demás (caret, selección de texto, scroll del documento, redimensionar, arrastrar la ventana...) se queda exactamente igual que ahora. No se anima nada dentro del documento en sí.

## Global Constraints

- Un commit por tarea terminada, con la línea `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`.
- `anim.rs` es lógica pura y se testea con `Instant`s fabricados a mano (`Instant::now() + Duration` no sirve para fijar el pasado; usa la técnica ya usada en el resto del workspace de pasar `now` como parámetro en vez de leer el reloj dentro de la función).
- Ninguna animación bloquea la interacción: si el usuario teclea o hace clic mientras algo está a medio animar, se gana la interacción — no hay "esperar a que termine la animación" en ningún sitio.
- Con `prefers-reduced-motion` no hay forma de leerlo en Win32 puro de forma trivial multiplataforma; en su lugar, respeta `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, ...)` — si el usuario tiene animaciones del sistema desactivadas en Windows (Accesibilidad → Efectos visuales), las animaciones de notty también se saltan (todo pasa a su estado final al instante). Consúltalo una vez al arrancar y cachéalo; no hace falta escuchar cambios en caliente.
- No optimices prematuramente: `anim.rs` no necesita ser genérico ni tener más de 2-3 curvas de easing. Si te encuentras diseñando un "sistema de animaciones" con timelines/keyframes, párate: no es lo que pide este plan.
- **Sobre el código Win32/Direct2D de este plan:** se da con nombres de API reales; ajusta firmas exactas contra el error del compilador o `cargo doc -p windows --open` sin cambiar el comportamiento descrito, como en los planes anteriores.
- Para la verificación visual (Tasks 3, 5, 6, 7): usa `tools/shot-app.ps1` y compara con `docs/mockups/ref/*.png` y capturas propias, con la herramienta Read. **Esta máquina tiene aplicaciones en primer plano (un juego, Discord, Steam) que a veces le roban el foco a mitad de una secuencia de teclas/clics** y arruinan una captura sin que sea un bug real de la app — si una captura sale claramente rara (contenido que no es de notty, ventana en blanco, texto de otra aplicación), reinténtalo 1-2 veces antes de darlo por un bug; si tras varios intentos no consigues una captura limpia, documéntalo como "no verificado por foco inestable" en tu resumen final y sigue adelante en vez de bloquearte ahí. Las animaciones en sí (el movimiento) no se pueden capturar en una imagen fija: verifica su *estado final* (a dónde llega) con capturas, y razona sobre la lógica de interpolación con los tests de `anim.rs` para la parte "en movimiento".

---

### Task 1: `anim.rs` — curva de interpolación (lógica pura)

**Files:**
- Create: `crates/notty-ui/src/anim.rs`
- Modify: `crates/notty-ui/src/lib.rs` (`mod anim; pub use anim::{Anim, ease_out_cubic};`)

- [x] **Step 1: Escribir los tests que fallan**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn progress_is_zero_at_start_and_one_when_done() {
        let start = Instant::now();
        let a = Anim::new(start, Duration::from_millis(100));
        assert_eq!(a.progress(start), 0.0);
        assert_eq!(a.progress(start + Duration::from_millis(100)), 1.0);
        assert_eq!(a.progress(start + Duration::from_millis(500)), 1.0); // no se pasa de 1
    }

    #[test]
    fn progress_is_linear_in_time_before_easing() {
        let start = Instant::now();
        let a = Anim::new(start, Duration::from_millis(100));
        assert_eq!(a.progress(start + Duration::from_millis(50)), 0.5);
    }

    #[test]
    fn is_done_matches_progress_reaching_one() {
        let start = Instant::now();
        let a = Anim::new(start, Duration::from_millis(100));
        assert!(!a.is_done(start + Duration::from_millis(99)));
        assert!(a.is_done(start + Duration::from_millis(100)));
    }

    #[test]
    fn ease_out_cubic_starts_fast_and_settles() {
        // Ease-out: a mitad de tiempo ya se ha recorrido más de la mitad del camino.
        assert!(ease_out_cubic(0.5) > 0.5);
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
    }

    #[test]
    fn eased_value_interpolates_from_to() {
        let start = Instant::now();
        let a = Anim::new(start, Duration::from_millis(100));
        assert_eq!(a.value(start, 10.0, 20.0), 10.0);
        assert_eq!(a.value(start + Duration::from_millis(100), 10.0, 20.0), 20.0);
    }

    #[test]
    fn reduced_motion_skips_straight_to_the_end() {
        let start = Instant::now();
        let a = Anim::new_maybe(start, Duration::from_millis(100), false);
        assert!(a.is_done(start)); // sin animación: "terminada" desde el instante 0
        assert_eq!(a.value(start, 10.0, 20.0), 20.0);
    }
}
```

- [x] **Step 2: Ejecutar y ver que falla**

Run: `cargo test -p notty-ui anim`
Expected: FAIL de compilación, `cannot find type Anim`.

- [x] **Step 3: Implementar**

```rust
//! Interpolación con el tiempo para las animaciones cortas de la interfaz (menús,
//! sugerencias, pestañas, el interruptor de Ajustes...). Sin acceso al reloj: todo
//! recibe `now` como parámetro, para poder testear con instantes fijos.

use std::time::{Duration, Instant};

/// Ease-out cúbico: arranca rápido y frena hacia el final, la sensación estándar de
/// "algo que aparece" en vez de un lineal (que se siente mecánico) o un ease-in
/// (que se siente lento de arrancar, mal para algo que el usuario acaba de pedir).
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[derive(Debug, Clone, Copy)]
pub struct Anim {
    start: Instant,
    duration: Duration,
    /// Si es `false`, `progress`/`value` devuelven el estado final desde `start`
    /// (animaciones del sistema desactivadas en Windows: ver `system_animations_enabled`).
    enabled: bool,
}

impl Anim {
    pub fn new(start: Instant, duration: Duration) -> Self {
        Self { start, duration, enabled: true }
    }

    pub fn new_maybe(start: Instant, duration: Duration, enabled: bool) -> Self {
        Self { start, duration, enabled }
    }

    /// `0.0..=1.0` lineal en el tiempo transcurrido, sin la curva de easing.
    pub fn progress(&self, now: Instant) -> f32 {
        if !self.enabled || self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.start).as_secs_f32();
        (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn is_done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    /// Valor interpolado entre `from` y `to`, con la curva de easing aplicada.
    pub fn value(&self, now: Instant, from: f32, to: f32) -> f32 {
        let t = ease_out_cubic(self.progress(now));
        from + (to - from) * t
    }
}
```

- [x] **Step 4: Ejecutar y ver que pasa**

Run: `cargo test --workspace`
Expected: PASS, +6 tests.

- [x] **Step 5: Commit** `feat(ui): anim.rs, interpolación con easing para las animaciones cortas`

---

### Task 2: Detectar si las animaciones del sistema están activadas

**Files:**
- Modify: `crates/notty-ui/src/window.rs`

- [x] **Step 1:** Añade una función que consulte `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut BOOL, 0)` (constantes en `windows::Win32::UI::WindowsAndMessaging`; si la constante exacta no existe con ese nombre en `windows` 0.62.2, usa su valor numérico `0x1042` con `SystemParametersInfoW` genérico — documenta cuál de las dos rutas usaste). Si la llamada falla, asume `true` (animar por defecto; no es un caso que deba silenciar la interfaz).

```rust
/// Si el usuario desactivó las animaciones del sistema (Accesibilidad → Efectos
/// visuales), las de notty también se saltan. Se consulta una vez al arrancar la
/// ventana y se guarda en `WindowState`; no hace falta escuchar cambios en caliente.
fn system_animations_enabled() -> bool {
    unsafe {
        let mut enabled = windows::Win32::Foundation::BOOL(1);
        let ok = windows::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(
            windows::Win32::UI::WindowsAndMessaging::SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut enabled as *mut _ as *mut core::ffi::c_void),
            Default::default(),
        );
        ok.is_err() || enabled.as_bool()
    }
}
```

- [x] **Step 2:** Guarda el resultado en `WindowState.animations_enabled: bool`, calculado una vez en `run()` al crear el estado.
- [x] **Step 3:** Compila (`cargo build --workspace`). No hay test automático razonable para esto (depende de configuración real de Windows); verifícalo leyendo el código, no hace falta probarlo a mano.
- [x] **Step 4: Commit** `feat(ui): respeta si el usuario desactivó las animaciones del sistema`

---

### Task 3: Menú, sugerencias y desplegable de Ajustes — fundido + desplazamiento al abrir

**Files:**
- Modify: `crates/notty-ui/src/window.rs` (estado + `SetTimer`/`KillTimer` del temporizador de animación)
- Modify: `crates/notty-ui/src/render.rs` (usa el progreso de la animación al dibujar)
- Modify: `crates/notty-ui/src/theme.rs` (helper de alpha)

- [x] **Step 1: Helper de alpha en `theme.rs`**

```rust
impl Rgba {
    /// Mismo color con la opacidad multiplicada por `factor` (`0.0..=1.0`), para
    /// fundidos: `Rgba(r,g,b,a).faded(0.4)` da `Rgba(r,g,b,a*0.4)`.
    pub fn faded(self, factor: f32) -> Self {
        Self(self.0, self.1, self.2, self.3 * factor.clamp(0.0, 1.0))
    }
}
```

Test en `theme.rs`: `faded(0.5)` sobre `Rgba(1.0,1.0,1.0,1.0)` da alpha `0.5`; `faded(1.0)` no cambia nada.

- [x] **Step 2: Estado de la animación del menú/sugerencias en `WindowState`**

Añade a `WindowState`:

```rust
/// Animación en curso del menú/sugerencias/desplegable de ruta que se acaba de
/// abrir (fundido + desplazamiento). `None` en reposo. Se limpia sola cuando
/// `is_done`, no hace falta borrarla al cerrar el menú (al cerrarse ya no se dibuja).
popup_open_anim: Option<(Instant, crate::Anim)>,
```

(El primer `Instant` es una marca de "para qué apertura es esta animación": basta con guardar solo el `Anim`, simplifícalo a `Option<crate::Anim>` si no necesitas distinguir qué se abrió — un único campo sirve porque el menú, las sugerencias y el desplegable de Ajustes nunca están abiertos dos a la vez.)

Al abrir cualquiera de los tres (donde ya se hace `w.open_menu = Some(i)`, donde se crea `Prompt::Path`/`Prompt::Find` con sugerencias, o donde ya hay lógica que abre el desplegable de Ajustes — este último vive en `settings_window.rs`, tarea aparte):

```rust
w.popup_open_anim = Some(crate::Anim::new_maybe(std::time::Instant::now(), std::time::Duration::from_millis(120), w.animations_enabled));
ensure_anim_timer(w, hwnd);
```

- [x] **Step 3: Temporizador de animación**

Constante nueva junto a `ID_AUTOSAVE_TIMER`/`ID_IPC_TIMER`:

```rust
const ID_ANIM_TIMER: usize = 3;
```

```rust
/// Arranca el temporizador de animación (60Hz) si no estaba ya corriendo. Se llama
/// cada vez que arranca una animación nueva.
fn ensure_anim_timer(w: &mut WindowState, hwnd: HWND) {
    if !w.anim_timer_running {
        unsafe { let _ = SetTimer(Some(hwnd), ID_ANIM_TIMER, 16, None); }
        w.anim_timer_running = true;
    }
}
```

En el manejador de `WM_TIMER` ya existente (donde se comprueba `ID_AUTOSAVE_TIMER`/`ID_IPC_TIMER`), añade el caso de `ID_ANIM_TIMER`:

```rust
} else if wparam.0 == ID_ANIM_TIMER {
    let now = std::time::Instant::now();
    let still_animating = w.popup_open_anim.as_ref().is_some_and(|a| !a.is_done(now));
    if !still_animating {
        w.popup_open_anim = None;
        unsafe { let _ = KillTimer(Some(hwnd), ID_ANIM_TIMER); }
        w.anim_timer_running = false;
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
}
```

Añade `anim_timer_running: bool` a `WindowState` (arranca en `false`).

- [x] **Step 4: Usarlo al dibujar**

`Renderer::paint`/`ViewState` necesita saber el progreso actual: añade `pub popup_open: Option<f32>` a `ViewState` (el valor de `w.popup_open_anim.map(|a| a.value(Instant::now(), 0.0, 1.0))`, calculado en `view_state()` donde ya se construye `ViewState` cada `WM_PAINT`).

En `draw_dropdown` (menú de la barra) y en `draw_suggestions` (línea de ruta): si `view.popup_open` es `Some(t)` con `t < 1.0`, en vez de dibujar el fondo/filas en su sitio final, desplázalos verticalmente `(1.0 - t) * 4.0` píxeles hacia arriba (o hacia su origen, según convenga) y funde todos los colores de esa caja con `.faded(t)` (el fondo `chrome_hi`, la sombra, el texto de cada fila). No hace falta animar el hit-testing: los rectángulos de `hits` se registran en su posición final aunque la caja todavía se esté desplazando 4px — un clic a mitad de la animación (120ms) cae dentro de un margen de error aceptable, no merece la complejidad de animar también el hit-test.

- [x] **Step 5: Compilar y capturar**

`cargo build --release`, abre el menú Archivo/Ver un momento antes de capturar (con `-Keys` puedes forzar la apertura pero no capturar a mitad de animación de forma fiable; compara el estado **final** — 150ms después de abrir, ya asentado — contra cómo se veía antes: debe verse igual de nítido, la animación es solo de entrada).

- [x] **Step 6: Commit** `feat(ui): el menú, las sugerencias y el desplegable funden y se deslizan al abrir`

---

### Task 4: Pestaña activa — fundido al cambiar

**Files:**
- Modify: `crates/notty-ui/src/window.rs`
- Modify: `crates/notty-ui/src/render.rs`

- [x] **Step 1:** Añade `tab_switch_anim: Option<crate::Anim>` a `WindowState`. Donde ya se cambia `ws.activate(i)` / `ws.next()` / `ws.prev()` (clic en pestaña, `Ctrl+Tab`, `Ctrl+Shift+Tab`), arranca la animación igual que en la Task 3 (`Duration::from_millis(100)`) y llama a `ensure_anim_timer`. Añade su comprobación de `is_done` junto a la de `popup_open_anim` en el `WM_TIMER` de la Task 3 (un único temporizador sirve para todas las animaciones: el `WM_TIMER` sigue corriendo mientras *cualquiera* de los campos de animación esté activo, y se para solo cuando *todos* han terminado).
- [x] **Step 2:** En `draw_tabs_row` (`render.rs`), añade `tab_switch: Option<f32>` a `ViewState` igual que `popup_open`. Al pintar la pestaña activa, en vez de `pal.surface` a opacidad plena desde el primer frame, interpola su alpha de fondo desde 0 hasta la de `pal.surface` con el progreso de `tab_switch` (si es `None` o ya vale `1.0`, se pinta igual que ahora, sin coste extra).
- [x] **Step 3:** Compilar, capturar el estado final tras cambiar de pestaña (debe verse idéntico a antes de este plan).
- [x] **Step 4: Commit** `feat(ui): la pestaña activa funde su fondo al cambiar en vez de aparecer de golpe`

---

### Task 5: Ajustes — interruptor deslizante y ventana con fundido+escala al abrir

**Files:**
- Modify: `crates/notty-ui/src/settings_window.rs`

**Contexto de esta ventana:** tiene su propio `State`/`wndproc`/bucle de mensajes (Plan 5b la dejó como una ventana Win32 aparte de la principal, con su propio `Renderer`). Todo lo de esta tarea vive dentro de ese archivo; no toca `window.rs`.

- [ ] **Step 1: Interruptor deslizante**

Añade a `State`: `toggle_anims: std::collections::HashMap<usize, crate::Anim>` (una animación por fila de `Toggle`, indexada por su posición en la sección actual — se puede limpiar entera al cambiar de sección). Al alternar un `Toggle` (`Hit::Toggle(row)`, donde ya se llama a `setting::apply`/se guarda), arranca/reinicia `toggle_anims.insert(row, Anim::new_maybe(Instant::now(), Duration::from_millis(140), animations_enabled))` con `from`/`to` según el nuevo estado, y arranca el temporizador de esta ventana (mismo patrón `ensure_anim_timer`/`ID_ANIM_TIMER`, pero local a `settings_window.rs` — esta ventana ya tiene su propio `WM_TIMER` o hay que añadirlo si no lo tenía; revisa el código real y adáptalo).

Al dibujar cada `Toggle` (donde ya se calcula la posición de la bolita a partir de si está activado): si hay una animación en curso para esa fila, usa `anim.value(now, x_off, x_on)` para la `x` de la bolita en vez de saltar directamente a la posición final; el fondo del interruptor (el color de la pista) puede fundir igual con `faded`/interpolación de color simple (mezcla lineal componente a componente entre el color apagado y el encendido, ya que no hay helper de mezcla de color: `fn lerp_color(a: Rgba, b: Rgba, t: f32) -> Rgba` trivial, dos o tres líneas).

- [ ] **Step 2: Ventana de Ajustes con fundido+escala al abrir**

En `settings_window::open`, la ventana ya se crea y se muestra (`ShowWindow`); en vez de que el primer `WM_PAINT` dibuje directamente al 100%, arranca una animación de apertura (`Duration::from_millis(150)`) nada más crear la ventana, guardada en `State`. Mientras esté activa, en el `paint()` de esta ventana: (a) funde todo el contenido con `faded(t)` sobre el mismo fondo ya opaco (no hace falta tocar la opacidad de la ventana en sí vía Win32, con fundir los colores dibujados basta para el efecto visual), y (b) aplica una escala uniforme del 97% al 100% centrada en el centro de la ventana a las coordenadas antes de dibujar — esto último es más delicado con las primitivas actuales (todo se dibuja con rectángulos/texto en coordenadas absolutas, no hay una transformación global): la forma más simple de conseguirlo sin reescribir cada función de dibujo es aplicar una transformación de mundo a Direct2D antes de `BeginDraw` (`self.target.SetTransform(&matrix)`, con una `Matrix3x2` de escala+traslación) y `SetTransform(&Matrix3x2::identity())` al terminar — así todo lo que se dibuje ese frame queda escalado sin tocar ninguna coordenada a mano. Si `SetTransform` no está disponible o da problemas con el hit-testing (los rectángulos guardados en `hits` seguirían en coordenadas sin escalar, lo cual es correcto: el ratón se compara contra las coordenadas lógicas, no contra lo que se ve escalado un instante), documenta la desviación y simplifica a solo el fundido (sin escala) si el `SetTransform` complica demasiado esta tarea — el fundido solo ya es una mejora notable sobre el "pop" actual.

- [ ] **Step 3:** Compilar, abrir Ajustes y capturar su estado final (debe verse idéntico a antes).
- [ ] **Step 4: Commit** `feat(ui): interruptor deslizante y Ajustes funde+escala al abrir`

---

### Task 6: Ajustes — marca de selección en el desplegable abierto

**Files:**
- Modify: `crates/notty-ui/src/settings_window.rs`

Bug de pulido encontrado al revisar `draw` del desplegable abierto (`Row::Select`): las filas de las opciones no marcan cuál es la actualmente seleccionada (solo se resalta la que tiene el ratón encima, `Hit::SelectOption`). En la maqueta (`.menu-item kbd`, fila con `✓`) y en cualquier desplegable nativo de Windows, la opción activa lleva una marca aunque el ratón esté sobre otra.

- [ ] **Step 1:** En el bucle que dibuja las filas del desplegable abierto, calcula `selected = selected_option_index(&st.cfg.borrow(), key, options)` (la misma función que ya usa `Row::Select` para pintar la etiqueta cerrada) y, para la fila `j == selected`, añade un `✓` a la izquierda del texto (o cambia su color a `pal.accent`, lo que quede más consistente con el resto de la app — mira cómo la maqueta marca la fila seleccionada de las sugerencias de ruta, `.prow.sel`, y replica ese mismo lenguaje visual: fondo `accent_soft` + texto `accent`, en vez de limitarte al hover).
- [ ] **Step 2:** Compilar, abrir un desplegable de Ajustes (p.ej. "Posición de las pestañas") y capturar: la opción activa debe distinguirse de las demás incluso sin el ratón encima.
- [ ] **Step 3: Commit** `fix(ui): el desplegable de Ajustes marca cuál es la opción actual`

---

### Task 7: Repaso visual de Ajustes — segmentados, botones, fila de navegación

**Files:**
- Modify: `crates/notty-ui/src/settings_window.rs`

Pulido general, sin lógica nueva: compara `docs/mockups/ref/ajustes.png` contra una captura fresca de la ventana real (`tools/shot-app.ps1`, o mejor: como esta ventana no está cubierta por el CLI de `notty.exe` directamente, ábrela con `^,` desde una ventana principal ya abierta) y corrige lo que no encaje.

- [ ] **Step 1:** Segmentado (`Row::Seg`, p.ej. Preset/Tema): comprueba que la opción marcada tiene fondo `pal.surface` + borde de 1px `pal.line` (no solo texto en negrita) y que el hover de una opción no marcada usa `pal.hover`, como la maqueta (`.seg button[aria-pressed="true"]`). Corrige si alguno de los dos estados falta o usa un color distinto.
- [ ] **Step 2:** Fila de navegación de la izquierda (Apariencia/Ventana/Teclado/Archivos/Atajo global): confirma que la sección activa lleva la barra de acento de 3px a la izquierda (`::before` de la maqueta) y que el resto tiene suficiente espacio de hover sin overlap entre filas.
- [ ] **Step 3:** Botones de texto tipo enlace (`Row::Link`, "Editar el archivo"/"Abrir [keys]"): confirma que tienen algún estado de hover (subrayado o cambio de color), no solo el color de acento estático — la maqueta los subraya al pasar el ratón (`.snav .foot a:hover{text-decoration:underline}`).
- [ ] **Step 4:** Compilar, capturar cada sección de Ajustes una vez y comparar con `ajustes.png`.
- [ ] **Step 5: Commit** `chore(ui): repaso visual de segmentados, navegación y enlaces en Ajustes`

---

### Task 8: Comprobación final

- [ ] **Step 1:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --release --workspace`: todo limpio.
- [ ] **Step 2:** Repasa mentalmente (o con capturas si el foco lo permite) que ninguna animación bloquea la escritura: abre el menú y escribe/haz clic antes de que termine de fundir, confirma que la acción se aplica igual que si no hubiera animación.
- [ ] **Step 3:** Confirma que el `WM_TIMER` de animación de cada ventana se para de verdad en reposo (no hay ninguna animación activa) — revisa el código, no hace falta instrumentación especial.
- [ ] **Step 4: Commit** `chore(ui): repaso final tras animaciones y pulido de Ajustes` (`--allow-empty` si no hubo cambios de esta revisión).
