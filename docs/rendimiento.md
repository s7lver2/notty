# Rendimiento: antes y después (2026-09-28)

Esta pasada de optimización cubre el arranque, la apertura de archivos, el consumo en reposo, la memoria y el
coste de escribir y buscar en archivos grandes. Las animaciones no se han tocado: siguen a 60 Hz con las
mismas curvas y duraciones, y el tiempo por frame se mantiene igual o mejora (tabla de scroll más abajo).

## Cómo medir

Hay dos benchmarks:

- **De punta a punta**: `tools/bench.ps1`. Arranca el `notty.exe` real en un perfil temporal (su propio
  `%APPDATA%` y `%LOCALAPPDATA%`, sin tocar tu configuración ni reenviar archivos a otra ventana abierta).
  Mide: el tiempo hasta el primer pintado (incluye abrir el archivo), la memoria, la CPU en 10 s de reposo, y
  120 pasos de rueda a ~60 Hz.

  ```
  tools/bench.ps1 -Exe target\release\notty.exe -Runs 5 -Out resultados.json
  tools/bench.ps1 -Scenarios vacio,log_50MB      # solo algunos escenarios
  ```

- **Micro-benchmarks** (sin ventana): `cargo run --release -p notty-ui --example bench`. Miden abrir, resaltar,
  analizar Markdown, buscar y copiar texto.

La instrumentación solo se activa con la variable `NOTTY_BENCH_LOG=<archivo>`
(`crates/notty-ui/src/bench_log.rs`). Escribe una línea por pintado (`p <unix_ms> <µs>`) y marcas de las fases
del arranque (`m <unix_ms> <fase>`). Sin esa variable no hace nada.

Archivos de prueba, generados en `%TEMP%\notty-bench`:

| Escenario  | Archivo                                               |
|------------|-------------------------------------------------------|
| `vacio`    | sin archivo                                           |
| `rust_1MB` | 1 MB de código Rust (resaltado de sintaxis)           |
| `log_10MB` | 10 MB de log de texto                                 |
| `log_50MB` | 50 MB de log de texto                                 |
| `md_1MB`   | 1 MB de Markdown, abierto en Previsualización         |

Los datos crudos están en `docs/bench/` (`antes.json`, `despues.json`, `micro-antes.txt`, `micro-despues.txt`).

Máquina: Windows 11 Pro, build release (`opt-level = "s"`, LTO). Todas las cifras son la **mediana de 5
ejecuciones**. La CPU se mide en millones de ciclos (`QueryProcessCycleTime`), porque `TotalProcessorTime`
va a saltos de 15,6 ms y no sirve para cantidades tan pequeñas.

## Resultados de punta a punta

### Arranque: hasta el primer pintado, con el archivo ya abierto

| Escenario  | Antes  | Después | Mejora       |
|------------|-------:|--------:|--------------|
| vacío      | 362 ms |   67 ms | 5,4× más rápido |
| rust 1 MB  | 787 ms |   94 ms | 8,4× más rápido |
| log 10 MB  | 378 ms |   74 ms | 5,1× más rápido |
| log 50 MB  | 481 ms |  122 ms | 3,9× más rápido |
| md 1 MB    | 638 ms |  162 ms | 3,9× más rápido |

### Reposo: 10 s sin tocar nada

| Escenario  | Repintados antes | Repintados después | CPU antes     | CPU después  |
|------------|-----------------:|-------------------:|--------------:|-------------:|
| vacío      | 10               | 0                  | 109 Mciclos   | 36 Mciclos   |
| rust 1 MB  | 10               | 0                  | 152 Mciclos   | 36 Mciclos   |
| log 10 MB  | 10               | 0                  | 134 Mciclos   | 36 Mciclos   |
| log 50 MB  | 10               | 0                  | 121 Mciclos   | 35 Mciclos   |
| md 1 MB    | 10               | 0                  | 158 Mciclos   | 34 Mciclos   |

### Memoria privada, 1,5 s después de abrir

| Escenario  | Antes    | Después  | Diferencia |
|------------|---------:|---------:|-----------:|
| vacío      |  55,3 MB |  63,2 MB | +7,9 MB    |
| rust 1 MB  | 115,3 MB | 122,4 MB | +7,1 MB    |
| log 10 MB  |  83,5 MB |  78,6 MB | −4,9 MB    |
| log 50 MB  | 177,5 MB | 125,2 MB | −52,3 MB   |
| md 1 MB    |  82,3 MB |  84,7 MB | +2,4 MB    |

Los ~7 MB de más en los archivos pequeños son lo que deja cargado el pintado por software del primer frame (ver
"Arranque" más abajo). A cambio, la ventana sale ~300 ms antes. Con archivos grandes ese coste queda de sobra
compensado.

### Scroll: 120 pasos de rueda a ~60 Hz

| Escenario  | Frame medio antes | Frame medio después | p95 antes | p95 después | CPU antes | CPU después |
|------------|------------------:|--------------------:|----------:|------------:|----------:|------------:|
| vacío      | 1,12 ms           | 1,07 ms             | 1,45 ms   | 1,58 ms     | 643 Mc    | 608 Mc      |
| rust 1 MB  | 2,30 ms           | 2,13 ms             | 2,97 ms   | 2,69 ms     | 1143 Mc   | 1041 Mc     |
| log 10 MB  | 1,71 ms           | 1,46 ms             | 2,22 ms   | 1,96 ms     | 898 Mc    | 777 Mc      |
| log 50 MB  | 1,51 ms           | 1,46 ms             | 1,92 ms   | 1,88 ms     | 796 Mc    | 784 Mc      |
| md 1 MB    | 2,59 ms           | 1,70 ms             | 3,41 ms   | 2,41 ms     | 1273 Mc   | 859 Mc      |

## Resultados de los micro-benchmarks

| Medida                                                       | Antes     | Después   |
|--------------------------------------------------------------|----------:|----------:|
| Abrir un log de 50 MB (leer + decodificar + rope)            | 138,0 ms  |  60,9 ms  |
| Abrir un log de 10 MB                                        |  26,4 ms  |  16,9 ms  |
| Primer resaltado de un .rs de 1 MB: bloqueo de la UI         | 369,4 ms  |   1,2 ms  |
| Primer resaltado de un .rs de 1 MB: hasta tener colores      | 369,4 ms  | 360,3 ms  |
| Teclear 1 carácter en un .rs de 1 MB: bloqueo de la UI       |  24,2 ms  |   1,4 ms  |
| `markdown::analyze` de 1 MB                                  | 199,6 ms  |  50,1 ms  |
| Extraer las líneas de un documento de 1 MB                   |  71,6 ms  |  16,3 ms  |
| Buscador abierto en 50 MB: coste por pintado                 | 241,6 ms  |   0,0 ms  |
| Buscar de cero en 50 MB                                      | 241,6 ms  | 133,8 ms  |
| Snapshot de recuperación en 50 MB (cada segundo)             |  33,3 ms  |   0,0 ms  |

## Qué se cambió

### Arranque

- **Primer frame pintado por CPU.** La primera vez, crear el render target de Direct2D por GPU tardaba ~285 ms
  (carga del driver): era casi todo el arranque. La ventana principal arranca ahora con un render target por
  software (~20 ms) y pasa a la GPU justo después del primer pintado (`Renderer::new_software` y
  `upgrade_to_gpu`, `WM_GPU_READY`). La imagen es la misma.
- Probé también a cargar el driver en otro hilo mientras tanto. Terminaba en el mismo momento y dejaba ~12 MB
  más en memoria, así que lo descarté.
- La ventana se crea directamente al tamaño final con el DPI del sistema, en vez de crearla y redimensionarla.

### Abrir archivos

- **UTF-8 sin copia.** Se valida el buffer leído del disco y se reutiliza como `String`, en vez de copiarlo
  entero (`notty-io/src/open.rs`).
- **Detección de fin de línea con SIMD** (`memchr`, que ya estaba en el árbol de dependencias por `regex`).
  Antes eran dos búsquedas de subcadena: ~38 ms en 50 MB, ahora ~6 ms.
- **Rope construido en paralelo** a partir de 4 MB: se trocea en frontera de carácter (nunca entre el `\r` y el
  `\n` de un CRLF), se construye cada trozo en un hilo y se unen con `Rope::append`. En 50 MB baja de ~46 ms a
  ~14 ms.

### Resaltado de sintaxis (`syntax.rs`)

- A partir de 128 KB, tree-sitter parsea en otro hilo. Tanto el primer parseo (~360 ms con 1 MB) como el
  reparseo incremental de cada tecla (~21 ms) dejan de bloquear la interfaz.
- Mientras tanto se sigue pintando con el último árbol válido. Cuando llega el nuevo, la ventana se repinta
  (`WM_SYNTAX_READY`).
- Los tramos de color salen del índice de líneas del propio árbol, no del documento, así que un árbol que va
  una tecla por detrás no puede leer fuera de rango.

### Markdown

- Los bloques de código se colorean la primera vez que se ven, no todos al abrir: con miles de bloques, eso era
  lo que más tardaba.
- Las líneas se extraen del rope en una sola pasada (`Buffer::line_strings`).
- Los anchos de columna de las tablas se guardan en caché. Antes se medían recorriendo el documento entero en
  cada frame.

### Reposo

- El temporizador de autoguardado (1 Hz) solo repinta si de verdad guardó algo. Antes repintaba la ventana
  entera cada segundo.
- El pipe de instancia única ya no se sondea cada 150 ms. El hilo que recibe despierta la ventana con un
  mensaje (`WM_IPC`), y lo mismo hacen la descarga de actualizaciones (con la barra de progreso limitada a
  ~30 repintados por segundo) y "Buscar ahora".

### Memoria

- El snapshot de recuperación (para no perder nada si notty se cuelga) guarda un clon del rope, que es O(1) y
  comparte memoria con el documento. Antes copiaba el texto entero de cada pestaña cada segundo: en un archivo
  de 50 MB eran 50 MB más y ~33 ms de CPU por segundo.

### Búsqueda

- Los resultados se guardan en caché por (revisión del documento, texto buscado, opciones). Antes se volvía a
  buscar en cada pintado con el buscador abierto, y en cada pulsación del contador `1/N`.
- Las posiciones de byte se pasan a posiciones de carácter en una sola pasada, en vez de consultar el rope por
  cada coincidencia.

## Lo que queda (medido, no arreglado)

- **Buscar de cero en 50 MB** sigue costando ~134 ms por cada letra que se escribe en el buscador. La mitad es
  copiar el documento a un `String` contiguo para `regex`.
- **Markdown en modo "En línea"** vuelve a analizar el documento entero en cada tecla: ~50 ms con 1 MB,
  ~2 ms con un README normal.
- **Paso a GPU**: tras el primer frame, el hilo de la interfaz queda ~280 ms ocupado cargando el driver. Las
  teclas pulsadas en ese rato se encolan, no se pierden.
- **`opt-level = 3`** en vez de `"s"`: probado. Solo gana ~10 % en parseo y análisis de Markdown (que ya van
  en segundo plano o son baratos) y engorda el `.exe` 1,2 MB, así que no se ha cambiado.
