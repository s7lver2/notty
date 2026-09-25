use crate::path_prompt::PathPromptState;
use crate::search_prompt::SearchState;

#[derive(Default)]
pub enum Prompt {
    #[default]
    None,
    Path(PathPromptState),
    Find(SearchState),
    Replace(SearchState),
    /// Línea de comandos vim (`:w`, `:q`, `:%s/a/b/g`, ...), abierta con `:` desde
    /// `VimState`. Guarda solo lo tecleado tras los dos puntos; ver `vim_cmd::parse_vim_cmd`.
    VimCmdline(String),
    /// El archivo cambió en disco desde que se abrió y se intentó guardar encima.
    /// Sin datos propios: `ws.active()` ya tiene ruta y texto en memoria, y basta
    /// con releer el disco al resolver.
    Conflict,
}
