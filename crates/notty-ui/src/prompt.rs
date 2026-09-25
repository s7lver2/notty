use crate::path_prompt::PathPromptState;
use crate::search_prompt::SearchState;

#[derive(Default)]
pub enum Prompt {
    #[default]
    None,
    Path(PathPromptState),
    Find(SearchState),
    Replace(SearchState),
}
