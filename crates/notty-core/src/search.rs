use std::ops::Range;

use regex::{NoExpand, Regex, RegexBuilder};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SearchError {
    #[error("regex no válida: {0}")]
    BadRegex(String),
}

fn build(query: &str, opts: SearchOptions) -> Result<Regex, SearchError> {
    let mut pattern = if opts.regex { query.to_string() } else { regex::escape(query) };
    if opts.whole_word {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    RegexBuilder::new(&pattern)
        .case_insensitive(!opts.case_sensitive)
        .build()
        .map_err(|e| SearchError::BadRegex(e.to_string()))
}

/// Coincidencias como rangos de **bytes** sobre `text`. Ignora coincidencias vacías.
pub fn find_all(text: &str, query: &str, opts: SearchOptions) -> Result<Vec<Range<usize>>, SearchError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = build(query, opts)?;
    Ok(re.find_iter(text).filter(|m| !m.is_empty()).map(|m| m.range()).collect())
}

pub fn replace_all(text: &str, query: &str, replacement: &str, opts: SearchOptions) -> Result<(String, usize), SearchError> {
    let count = find_all(text, query, opts)?.len();
    if count == 0 {
        return Ok((text.to_string(), 0));
    }
    let re = build(query, opts)?;
    let out = if opts.regex {
        re.replace_all(text, replacement).into_owned()
    } else {
        re.replace_all(text, NoExpand(replacement)).into_owned()
    };
    Ok((out, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> SearchOptions {
        SearchOptions::default()
    }

    #[test]
    fn default_search_ignores_case() {
        assert_eq!(find_all("Juan y juan", "juan", opts()).unwrap(), vec![0..4, 7..11]);
    }

    #[test]
    fn case_sensitive_search() {
        let o = SearchOptions { case_sensitive: true, ..opts() };
        assert_eq!(find_all("Juan y juan", "juan", o).unwrap(), vec![7..11]);
    }

    #[test]
    fn whole_word_search() {
        let o = SearchOptions { whole_word: true, ..opts() };
        assert_eq!(find_all("pan panadero pan", "pan", o).unwrap(), vec![0..3, 13..16]);
    }

    #[test]
    fn literal_by_default() {
        assert_eq!(find_all("a.b axb", "a.b", opts()).unwrap(), vec![0..3]);
    }

    #[test]
    fn regex_mode() {
        let o = SearchOptions { regex: true, ..opts() };
        assert_eq!(find_all("a1 b22", r"\d+", o).unwrap(), vec![1..2, 4..6]);
    }

    #[test]
    fn bad_regex_is_an_error() {
        let o = SearchOptions { regex: true, ..opts() };
        assert!(matches!(find_all("x", "(", o), Err(SearchError::BadRegex(_))));
    }

    #[test]
    fn empty_query_finds_nothing() {
        assert!(find_all("abc", "", opts()).unwrap().is_empty());
    }

    #[test]
    fn replace_all_literal_does_not_expand() {
        assert_eq!(replace_all("a b a", "a", "$1", opts()).unwrap(), ("$1 b $1".to_string(), 2));
    }

    #[test]
    fn replace_all_regex_expands_groups() {
        let o = SearchOptions { regex: true, ..opts() };
        assert_eq!(replace_all("2026-09", r"(\d+)-(\d+)", "$2/$1", o).unwrap(), ("09/2026".to_string(), 1));
    }
}
