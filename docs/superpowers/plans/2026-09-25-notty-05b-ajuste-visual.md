# notty · Plan 5b: Ajuste visual contra la maqueta Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que `notty.exe` se parezca de verdad a `docs/mockups/notty-ui.html` (preset Moderna, tema oscuro): jerarquía de texto (texto normal / secundario / atenuado, no todo del mismo blanco), pestañas con el fondo correcto (la activa se funde con el editor, las inactivas no llevan ningún tinte de acento), y una paleta de grises fríos coherente en vez de los tonos sueltos que se fueron añadiendo plan a plan.

**Architecture:** Cambio contenido en `crates/notty-ui/src/render.rs`: una paleta de constantes `D2D1_COLOR_F` más completa (con la jerarquía de texto que ya tiene la maqueta) y ajustes puntuales en los sitios donde hoy se pinta todo con `fg_brush` sin distinguir texto principal de secundario, y donde una pestaña usa por error el pincel de selección de texto. Ningún otro crate cambia. No hay lógica pura que testear aquí: es una pasada de estilo sobre código Win32/Direct2D ya existente, así que cada tarea termina en "compilar + comprobar a ojo", no en `cargo test`.

**Tech Stack:** Rust stable 1.96 (MSVC), `windows` (Direct2D, ya en el workspace). No se añaden dependencias.

**Spec visual de referencia:** `docs/mockups/notty-ui.html` (abrir en un navegador; usar el tema Oscuro, preset Moderna, que es el que arranca por defecto). Los valores OKLCH de ese archivo (bloque `:root[data-theme="dark"]`) son la fuente de verdad; las constantes `D2D1_COLOR_F` de este plan son una conversión aproximada a sRGB, ajustada a ojo contra la maqueta, no un cálculo colorimétrico exacto — si al comparar algo se ve claramente distinto, corrígelo a mano hasta que se parezca, no hace falta perseguir una coincidencia de píxel exacto.
**Diagnóstico de partida (por qué no se parecía):** dos causas confirmadas leyendo `crates/notty-ui/src/render.rs` tal como quedó tras los Planes 2-5:
1. La pestaña activa se rellena con `sel_brush` (el pincel de **resaltado de selección de texto**, un azul con alpha 0.35), en vez de con el color de fondo del editor. La maqueta hace justo lo contrario: la pestaña activa usa el mismo fondo que el editor (`--surface`), así que "desaparece" en el editor; solo las inactivas se distinguen, por estar sobre la tira (`--chrome`).
2. **Todo** el texto —menú, pestañas (activas e inactivas por igual), barra de atajos, barra de estado, números de línea— se dibuja con `fg_brush`, el mismo blanco brillante que el texto del documento. La maqueta usa una jerarquía deliberada (`--text` solo para el documento y la pestaña activa; `--text-2` para pestañas inactivas, atajos y barra de estado; `--text-3` para los números de línea y el texto atenuado) precisamente para que el documento sea lo único que llama la atención.

## Global Constraints

- Solo cambia `crates/notty-ui/src/render.rs` (y, si hace falta pasar algún dato nuevo como el índice de pestaña activa que ya se tiene, como mucho su firma interna; no toques `window.rs` ni ningún otro crate).
- No se añade Mica/tema/DPI nuevo: eso ya existe desde el Plan 2 y no es el problema reportado.
- Mantén los pinceles semánticos que sí están bien (`danger_brush`, `match_brush`, `match_current_brush`, `toggle_brush`, `ghost_brush`, `caret_brush`, `sel_brush` para selección de texto): este plan **añade** pinceles de jerarquía de texto y **corrige** el uso indebido de `sel_brush` en las pestañas, no reescribe lo que ya funciona.
- Un commit por tarea terminada.
- **Sobre el código de este plan:** es Direct2D/DirectWrite puro (igual que el resto de `render.rs`). Compílalo, y si una firma no coincide con la versión de `windows` instalada, ajústala consultando `cargo doc -p windows --open` o el error del compilador, sin cambiar el color/comportamiento descrito.

---

### Task 1: Paleta con jerarquía de texto

**Files:**
- Modify: `crates/notty-ui/src/render.rs`

**Interfaces:**
- Consumes: nada nuevo.
- Produces: tres constantes de color nuevas y sus pinceles, aproximando `--text-2`/`--text-3`/`--chrome` de la maqueta:

```rust
const TEXT_2: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.55, g: 0.55, b: 0.57, a: 1.0 };
const TEXT_3: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.30, g: 0.30, b: 0.32, a: 1.0 };
const CHROME: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.115, g: 0.115, b: 0.12, a: 1.0 };
```

y `pub struct Renderer` gana `text2_brush: ID2D1SolidColorBrush`, `text3_brush: ID2D1SolidColorBrush`, `chrome_brush: ID2D1SolidColorBrush`, creados en `Renderer::new` con el mismo patrón que ya usan `fg_brush`/`sel_brush`/etc. (`CreateSolidColorBrush` + `D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: Matrix3x2::identity() }`), y añadidos al `Self { ... }` final del constructor.

- [ ] **Step 1: Añadir las constantes y los tres pinceles**

En `crates/notty-ui/src/render.rs`, junto a las constantes de color ya existentes (`BG`, `FG`, `SEL`, `GHOST`, `DANGER`, `MATCH`, `MATCH_CURRENT`, `TOGGLE_ON`), añadir las tres nuevas de la Interfaz de arriba.

En `struct Renderer`, añadir los tres campos nuevos junto a `fg_brush`/`caret_brush`/etc.

En `Renderer::new`, tras la creación de `toggle_brush` (o en cualquier punto tras tener `target` disponible) y antes de `Ok(Self { ... })`, crear los tres pinceles nuevos con el mismo patrón que los demás, y añadirlos a la construcción de `Self`.

- [ ] **Step 2: Compilar**

Run: `cargo build --workspace`
Expected: compila (con avisos de "field never read" si todavía no se usan en este mismo commit — se usan en la Task 2, así que si prefieres evitar el aviso intermedio, haz las Tasks 1 y 2 en el mismo commit; si no, ignora el aviso temporal).

- [ ] **Step 3: Commit**

```bash
git add crates/notty-ui
git commit -m "feat(ui): paleta con jerarquía de texto (text-2/text-3/chrome)"
```

---

### Task 2: Corregir el color de las pestañas

**Files:**
- Modify: `crates/notty-ui/src/render.rs`

**Interfaces:**
- Consumes: `chrome_brush`, `text2_brush` (Task 1).
- Produces: la tira de pestañas dibuja primero una franja de fondo `chrome_brush` de ancho completo (para que las pestañas inactivas, que no se rellenan, se vean sobre ese tono en vez de sobre el `BG` del editor); la pestaña activa se rellena con `BG` (el mismo color que el editor: por eso "se funde" con él, igual que en la maqueta) en vez de `sel_brush`; el texto de la pestaña activa usa `fg_brush`, el de las inactivas `text2_brush`.

- [ ] **Step 1: Reescribir el bloque de pestañas**

Localizar en `crates/notty-ui/src/render.rs` el bloque `if show_tabs { ... }` (dentro de `paint`, tras `if show_menubar { ... }`). Sustituirlo por:

```rust
            if show_tabs {
                let tabs_top = top;
                self.target.FillRectangle(
                    &D2D_RECT_F { left: 0.0, top: tabs_top, right: size_before_tabs_width, bottom: tabs_top + self.line_height },
                    &self.chrome_brush,
                );
                let mut x = 0.0f32;
                for (i, doc) in ws.iter().enumerate() {
                    let name = match &doc.path {
                        Some(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                        None => "sin título".to_string(),
                    };
                    let dirty = if doc.doc.is_dirty() { " •" } else { "" };
                    let rect = D2D_RECT_F { left: x, top: tabs_top, right: x + TAB_WIDTH, bottom: tabs_top + self.line_height };
                    let (fill, text_brush) = if i == ws.active_index() {
                        (Some(&self.fg_brush), &self.fg_brush) // el "fill" real es BG, ver nota debajo
                    } else {
                        (None, &self.text2_brush)
                    };
                    if i == ws.active_index() {
                        self.target.FillRectangle(&rect, &self.bg_brush);
                    }
                    let _ = fill; // evita un warning si tu compilador se queja de la tupla anterior; puedes simplificar quitándola
                    self.draw_text_line_with(&format!("{name}{dirty}"), x + PADDING_X, tabs_top, TAB_WIDTH - PADDING_X, text_brush);
                    self.tab_rects.push((rect.left, rect.top, rect.right, rect.bottom));
                    x += TAB_WIDTH;
                }
                top += self.line_height;
            }
```

Este fragmento asume dos cosas que hay que resolver al aplicarlo, porque dependen de cómo haya quedado exactamente tu `render.rs` tras los planes anteriores:

1. **`bg_brush`**: hoy `BG` se usa solo en `self.target.Clear(Some(&BG))`, no hay un pincel `bg_brush` creado. Añádelo en la Task 1 en vez de aquí (un pincel más, mismo patrón, color `BG`) para poder usarlo en un `FillRectangle`. Actualiza el Step 1 de la Task 1 si prefieres añadirlo allí de una vez, o añádelo en esta misma tarea — cualquiera de las dos es correcta, pero **hazlo en un único sitio** y usa ese pincel aquí en vez de `fg_brush` (el borrador de arriba tiene una tupla `(fill, text_brush)` mal resuelta a propósito, como recordatorio de que `fill` debe ser el pincel `BG`, no `fg_brush`; simplifícala una vez tengas `bg_brush` disponible, por ejemplo quitando la tupla y dejando solo el `if`/`else` para el color del texto).
2. **`size_before_tabs_width`**: el ancho de la franja de fondo debe ser el ancho real del cliente en ese momento. Ya tienes `let size = self.target.GetSize();` un poco más abajo en la función (tras el bloque de pestañas); muévelo (o solo la línea que obtiene `size`) a **antes** de este bloque para poder usar `size.width` aquí, y elimina la llamada duplicada más abajo si `size` ya no hace falta recalcularlo.

Deja el código final limpio (sin la tupla `(fill, text_brush)` de recordatorio ni el `let _ = fill;`): la versión definitiva debe ser simplemente "si es la activa, rellena con `bg_brush` y usa `fg_brush` para el texto; si no, no rellenes nada y usa `text2_brush`".

- [ ] **Step 2: Usar la jerarquía de texto en el resto de franjas**

En el mismo `paint`, cambiar estas líneas (localízalas por el texto que dibujan, ya citado en el diagnóstico):
- La línea del menú (`"Archivo   Editar   Buscar   Ver   Ayuda"`): pásala por `draw_text_line_with(..., &self.text2_brush)` en vez de `draw_text_line` (que usa `fg_brush`).
- Los números de línea (el `DrawTextLayout` de `num_layout`): usa `&self.text3_brush` en vez de `&self.fg_brush`.
- La barra de atajos (`crate::hints_text(...)`): `draw_text_line_with(..., &self.text2_brush)`.
- La barra de estado (`status_line`) y su franja de fondo: usa `&self.text2_brush` para el texto. Si la franja de la barra de estado no tiene ya un `FillRectangle` con `chrome_brush` (la maqueta le da un fondo ligeramente distinto al del editor, `--chrome`), añádelo antes de dibujar el texto.

No toques el color del texto del documento en sí (sigue en `fg_brush`, es correcto: es lo único que debe llamar la atención) ni el de los prompts (ruta/búsqueda), que ya usan `fg_brush`/`ghost_brush`/`danger_brush` correctamente según el Plan 4.

- [ ] **Step 3: Compilar y comprobar a ojo**

Run: `cargo build --workspace`
Expected: compila.

Run: `cargo run --bin notty -- <un archivo de texto con varias líneas y al menos dos pestañas abiertas con Ctrl+N>`
Expected (manual, comparar con `docs/mockups/notty-ui.html` en preset Moderna/tema Oscuro):
- La pestaña activa ya no se ve azul: tiene el mismo fondo que el editor, casi "invisible" salvo por el texto en blanco.
- Las pestañas inactivas se ven sobre una franja algo más clara que el editor (`chrome`), con el texto atenuado, no blanco puro.
- La barra de atajos y la barra de estado ya no son del mismo blanco que el texto del documento.
- Los números de línea son claramente más tenues que el propio texto.

Si algo se sigue viendo muy distinto de la maqueta (demasiado claro, demasiado oscuro, mal contraste), ajusta los valores de `TEXT_2`/`TEXT_3`/`CHROME`/`BG` a mano hasta que se parezca — son aproximaciones, no hay un valor "correcto" único.

- [ ] **Step 4: Commit**

```bash
git add crates/notty-ui
git commit -m "fix(ui): la pestaña activa ya no usa el color de selección de texto"
```

---

### Task 3: Comprobación final

**Files:** ninguno nuevo; solo verificación.

- [ ] **Step 1: Tests y lints de todo el workspace**

Run: `cargo test --workspace`
Expected: PASS, sin cambios respecto al recuento de antes de este plan (este plan no toca lógica, solo `render.rs`, que no tiene tests automáticos).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: sin avisos.

- [ ] **Step 2: Build release y comparación final**

Run: `cargo build --release --workspace`
Expected: compila sin errores.

Run: `cargo run --bin notty -- <archivo de prueba>` una vez más y compararlo con la maqueta abierta al lado.
Expected (manual): el "aire" general (qué tan oscuro, qué tanto contraste, qué se destaca y qué no) coincide con la maqueta en preset Moderna/tema Oscuro.

- [ ] **Step 3: Commit (si hiciste ajustes de color durante la comparación)**

```bash
git add -A
git commit -m "fix(ui): ajustes finos de paleta tras comparar con la maqueta" --allow-empty
```
