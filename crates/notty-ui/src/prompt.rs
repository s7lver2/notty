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
    /// Va ligado a su documento (por id), no a la pestaña activa.
    Conflict(crate::ConflictState),
    /// "Tiene cambios sin guardar" al cerrar uno o varios documentos (o la ventana).
    CloseUnsaved(crate::CloseRequest),
}
