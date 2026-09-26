//! Resaltado de sintaxis con tree-sitter: qué gramática toca según la extensión y,
//! para las líneas visibles, qué tramos van con qué color. No sabe de Direct2D: da
//! columnas UTF-16 (lo que espera `IDWriteTextLayout::SetDrawingEffect`).

use std::cell::RefCell;
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;

use notty_core::Document;
use tree_sitter::{InputEdit, Language, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::theme::{Palette, Rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    JavaScript,
    TypeScript,
    Tsx,
    Python,
    C,
    Cpp,
    Json,
    Toml,
    Markdown,
    Yaml,
}

const LANG_COUNT: usize = 11;

pub fn lang_for_path(path: &Path) -> Option<Lang> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => Lang::Rust,
        "js" | "mjs" | "cjs" | "jsx" => Lang::JavaScript,
        "ts" | "mts" | "cts" => Lang::TypeScript,
        "tsx" => Lang::Tsx,
        "py" | "pyw" => Lang::Python,
        "c" | "h" => Lang::C,
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Lang::Cpp,
        "json" => Lang::Json,
        "toml" => Lang::Toml,
        "md" | "markdown" => Lang::Markdown,
        "yaml" | "yml" => Lang::Yaml,
        _ => return None,
    })
}

/// Los nombres estándar de `tree-sitter-highlight` (los de su README), más
/// `boolean`/`escape`, que usan varias consultas, y los `text.*` de Markdown.
pub const NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "function",
    "function.builtin",
    "keyword",
    "module",
    "number",
    "operator",
    "property",
    "property.builtin",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "boolean",
    "escape",
    "text.title",
    "text.literal",
    "text.uri",
    "text.reference",
];

/// Igual que `tree-sitter-highlight`: de los nombres conocidos, el que tenga más
/// partes y todas estén en la captura (`function.method` → `function`).
fn resolve(capture: &str) -> Option<usize> {
    let parts: Vec<&str> = capture.split('.').collect();
    let mut best: Option<(usize, usize)> = None;
    for (i, name) in NAMES.iter().enumerate() {
        let n = name.split('.').count();
        if name.split('.').all(|p| parts.contains(&p)) && best.is_none_or(|(_, bn)| n > bn) {
            best = Some((i, n));
        }
    }
    best.map(|(i, _)| i)
}

pub fn color(pal: &Palette, highlight: usize) -> Option<Rgba> {
    Some(match NAMES[highlight] {
        "comment" => pal.syn_comment,
        "keyword" | "variable.builtin" | "tag" | "text.title" => pal.syn_keyword,
        "string" | "string.special" | "escape" | "text.literal" => pal.syn_string,
        "number" | "boolean" | "constant" | "constant.builtin" => pal.syn_number,
        "type" | "type.builtin" | "constructor" | "attribute" | "module" => pal.syn_type,
        "function" | "function.builtin" | "text.uri" | "text.reference" => pal.syn_function,
        _ => return None,
    })
}

struct Grammar {
    language: Language,
    query: Query,
    /// Por índice de captura de `query`, su posición en `NAMES` (o nada si no se pinta).
    highlights: Vec<Option<usize>>,
}

fn grammar(lang: Lang) -> Option<&'static Grammar> {
    static CELLS: [OnceLock<Option<Grammar>>; LANG_COUNT] = [const { OnceLock::new() }; LANG_COUNT];
    CELLS[lang as usize].get_or_init(|| build(lang)).as_ref()
}

fn build(lang: Lang) -> Option<Grammar> {
    use tree_sitter_javascript as js;
    use tree_sitter_typescript as ts;
    // Mismo orden que los `tree-sitter.json` de cada gramática: TypeScript y C++
    // extienden las consultas de JavaScript y C, y a igual nodo gana el primer patrón.
    let (language, src): (Language, String) = match lang {
        Lang::Rust => (tree_sitter_rust::LANGUAGE.into(), tree_sitter_rust::HIGHLIGHTS_QUERY.into()),
        Lang::JavaScript => (js::LANGUAGE.into(), [js::HIGHLIGHT_QUERY, js::JSX_HIGHLIGHT_QUERY].join("\n")),
        Lang::TypeScript => (ts::LANGUAGE_TYPESCRIPT.into(), [js::HIGHLIGHT_QUERY, ts::HIGHLIGHTS_QUERY].join("\n")),
        Lang::Tsx => {
            (ts::LANGUAGE_TSX.into(), [js::HIGHLIGHT_QUERY, js::JSX_HIGHLIGHT_QUERY, ts::HIGHLIGHTS_QUERY].join("\n"))
        }
        Lang::Python => (tree_sitter_python::LANGUAGE.into(), tree_sitter_python::HIGHLIGHTS_QUERY.into()),
        Lang::C => (tree_sitter_c::LANGUAGE.into(), tree_sitter_c::HIGHLIGHT_QUERY.into()),
        Lang::Cpp => {
            (tree_sitter_cpp::LANGUAGE.into(), [tree_sitter_c::HIGHLIGHT_QUERY, tree_sitter_cpp::HIGHLIGHT_QUERY].join("\n"))
        }
        Lang::Json => (tree_sitter_json::LANGUAGE.into(), tree_sitter_json::HIGHLIGHTS_QUERY.into()),
        Lang::Toml => (tree_sitter_toml_ng::LANGUAGE.into(), tree_sitter_toml_ng::HIGHLIGHTS_QUERY.into()),
        Lang::Markdown => (tree_sitter_md::LANGUAGE.into(), tree_sitter_md::HIGHLIGHT_QUERY_BLOCK.into()),
        Lang::Yaml => (tree_sitter_yaml::LANGUAGE.into(), tree_sitter_yaml::HIGHLIGHTS_QUERY.into()),
    };
    let query = Query::new(&language, &src).ok()?;
    let highlights = query.capture_names().iter().map(|n| resolve(n)).collect();
    Some(Grammar { language, query, highlights })
}

/// Por encima de esto no se resalta: el primer parseo de 2 MB ya ronda el medio
/// segundo en release (luego cada tecla son ~10 ms, eso sí).
const MAX_BYTES: usize = 2 * 1024 * 1024;

/// Un tramo de una línea con su color: columnas UTF-16 desde el inicio de la línea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub len: u32,
    pub highlight: usize,
}

struct Parsed {
    lang: Lang,
    revision: u64,
    parser: Parser,
    tree: Tree,
    text: String,
}

/// El árbol de la última vez que se pintó la pestaña. `RefCell` porque se rellena
/// al pintar, que solo tiene `&EditorState`.
#[derive(Default)]
pub struct SyntaxCache(RefCell<Option<Parsed>>);

impl SyntaxCache {
    /// Tramos de color de cada línea de `lines` (vacío si no hay gramática para
    /// `path`). Reparsea solo si el texto cambió desde la última vez, y entonces de
    /// forma incremental a partir del árbol anterior.
    pub fn line_spans(&self, doc: &Document, path: Option<&Path>, lines: Range<usize>) -> Vec<Vec<Span>> {
        let mut slot = self.0.borrow_mut();
        let Some((lang, g)) = path.and_then(lang_for_path).and_then(|l| Some((l, grammar(l)?))) else {
            *slot = None;
            return Vec::new();
        };
        let Some(p) = refresh(&mut slot, lang, g, doc) else {
            return Vec::new();
        };
        spans(p, g, doc, lines)
    }
}

fn refresh<'a>(slot: &'a mut Option<Parsed>, lang: Lang, g: &Grammar, doc: &Document) -> Option<&'a Parsed> {
    let fresh = slot.as_ref().is_some_and(|p| p.lang == lang && p.revision == doc.revision());
    if !fresh {
        let prev = slot.take().filter(|p| p.lang == lang);
        let text = doc.text();
        if text.len() > MAX_BYTES {
            return None;
        }
        *slot = Some(match prev {
            Some(mut p) => {
                p.tree.edit(&diff_edit(&p.text, &text));
                p.tree = p.parser.parse(&text, Some(&p.tree))?;
                p.text = text;
                p.revision = doc.revision();
                p
            }
            None => {
                let mut parser = Parser::new();
                parser.set_language(&g.language).ok()?;
                let tree = parser.parse(&text, None)?;
                Parsed { lang, revision: doc.revision(), parser, tree, text }
            }
        });
    }
    slot.as_ref()
}

/// El cambio entre `old` y `new` como un solo reemplazo (prefijo y sufijo comunes
/// fuera), que es lo que necesita `Tree::edit` para reparsear solo lo tocado.
fn diff_edit(old: &str, new: &str) -> InputEdit {
    let (a, b) = (old.as_bytes(), new.as_bytes());
    let mut start = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(start) {
        start -= 1;
    }
    let max_suffix = a.len().min(b.len()) - start;
    let mut suffix = a.iter().rev().zip(b.iter().rev()).take(max_suffix).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(a.len() - suffix) {
        suffix -= 1;
    }
    let (old_end, new_end) = (a.len() - suffix, b.len() - suffix);
    InputEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: point(a, start),
        old_end_position: point(a, old_end),
        new_end_position: point(b, new_end),
    }
}

fn point(text: &[u8], byte: usize) -> Point {
    let before = &text[..byte];
    let row = before.iter().filter(|&&c| c == b'\n').count();
    let column = byte - before.iter().rposition(|&c| c == b'\n').map_or(0, |i| i + 1);
    Point { row, column }
}

fn spans(p: &Parsed, g: &Grammar, doc: &Document, lines: Range<usize>) -> Vec<Vec<Span>> {
    let buf = doc.buffer();
    let total = buf.len_lines();
    // Mismo recorte que `render.rs`: la línea sin su salto final.
    let line_bytes: Vec<Range<usize>> = lines
        .filter(|&l| l < total)
        .map(|l| {
            let s = buf.char_to_byte(buf.line_start(l));
            let full_end = if l + 1 < total { buf.char_to_byte(buf.line_start(l + 1)) } else { p.text.len() };
            s..s + p.text[s..full_end].trim_end_matches(['\r', '\n']).len()
        })
        .collect();
    let (Some(first), Some(last)) = (line_bytes.first(), line_bytes.last()) else {
        return Vec::new();
    };

    // (inicio, fin, patrón, color). Se aplican en este orden y cada uno pisa al
    // anterior: los nodos de fuera antes que los de dentro, y a igual nodo el patrón
    // de menor índice el último (en `tree-sitter-highlight` gana el primero).
    let mut caps: Vec<(usize, usize, usize, usize)> = Vec::new();
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(first.start..last.end.max(first.start + 1));
    let mut it = cursor.captures(&g.query, p.tree.root_node(), p.text.as_bytes());
    while let Some((m, i)) = it.next() {
        let c = m.captures[*i];
        if let Some(hl) = g.highlights[c.index as usize] {
            let r = c.node.byte_range();
            caps.push((r.start, r.end, m.pattern_index, hl));
        }
    }
    caps.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));

    line_bytes
        .iter()
        .map(|lr| {
            let line = &p.text[lr.clone()];
            caps.iter()
                .filter(|c| c.0 < lr.end && c.1 > lr.start)
                .filter_map(|c| {
                    let (s, e) = (c.0.max(lr.start) - lr.start, c.1.min(lr.end) - lr.start);
                    let start = utf16_len(line.get(..s)?);
                    let len = utf16_len(line.get(s..e)?);
                    (len > 0).then_some(Span { start, len, highlight: c.3 })
                })
                .collect()
        })
        .collect()
}

fn utf16_len(s: &str) -> u32 {
    s.chars().map(char::len_utf16).sum::<usize>() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const ALL: [Lang; LANG_COUNT] = [
        Lang::Rust,
        Lang::JavaScript,
        Lang::TypeScript,
        Lang::Tsx,
        Lang::Python,
        Lang::C,
        Lang::Cpp,
        Lang::Json,
        Lang::Toml,
        Lang::Markdown,
        Lang::Yaml,
    ];

    fn name_at(spans: &[Span], col: u32) -> Option<&'static str> {
        spans.iter().rev().find(|s| col >= s.start && col < s.start + s.len).map(|s| NAMES[s.highlight])
    }

    #[test]
    fn extension_picks_the_grammar() {
        assert_eq!(lang_for_path(Path::new("a/b.rs")), Some(Lang::Rust));
        assert_eq!(lang_for_path(Path::new("x.JSX")), Some(Lang::JavaScript));
        assert_eq!(lang_for_path(Path::new("x.ts")), Some(Lang::TypeScript));
        assert_eq!(lang_for_path(Path::new("x.tsx")), Some(Lang::Tsx));
        assert_eq!(lang_for_path(Path::new("x.h")), Some(Lang::C));
        assert_eq!(lang_for_path(Path::new("x.hpp")), Some(Lang::Cpp));
        assert_eq!(lang_for_path(Path::new("Cargo.toml")), Some(Lang::Toml));
        assert_eq!(lang_for_path(Path::new("README.markdown")), Some(Lang::Markdown));
        assert_eq!(lang_for_path(Path::new("ci.yml")), Some(Lang::Yaml));
        assert_eq!(lang_for_path(Path::new("notas.txt")), None);
        assert_eq!(lang_for_path(Path::new("Makefile")), None);
    }

    #[test]
    fn every_grammar_and_query_loads() {
        for lang in ALL {
            assert!(grammar(lang).is_some(), "{lang:?}");
        }
    }

    #[test]
    fn capture_names_resolve_like_tree_sitter_highlight() {
        assert_eq!(resolve("function.method").map(|i| NAMES[i]), Some("function"));
        assert_eq!(resolve("function.builtin").map(|i| NAMES[i]), Some("function.builtin"));
        assert_eq!(resolve("punctuation.bracket").map(|i| NAMES[i]), Some("punctuation.bracket"));
        assert_eq!(resolve("none"), None);
    }

    #[test]
    fn rust_line_gets_keyword_string_and_comment() {
        let doc = Document::new("fn main() {\n    let s = \"hola\"; // saludo\n}\n", "\n");
        let cache = SyntaxCache::default();
        let spans = cache.line_spans(&doc, Some(Path::new("a.rs")), 0..3);
        assert_eq!(name_at(&spans[0], 0), Some("keyword"));
        assert_eq!(name_at(&spans[0], 3), Some("function"));
        assert_eq!(name_at(&spans[1], 4), Some("keyword"));
        assert_eq!(name_at(&spans[1], 13), Some("string"));
        assert_eq!(name_at(&spans[1], 22), Some("comment"));
    }

    #[test]
    fn unknown_extension_or_no_path_gives_nothing() {
        let doc = Document::new("fn main() {}", "\n");
        let cache = SyntaxCache::default();
        assert!(cache.line_spans(&doc, None, 0..1).is_empty());
        assert!(cache.line_spans(&doc, Some(Path::new("a.txt")), 0..1).is_empty());
    }

    #[test]
    fn columns_are_utf16_after_multibyte_text() {
        let doc = Document::new("// ñ😀\nlet x = 1;", "\n");
        let spans = SyntaxCache::default().line_spans(&doc, Some(Path::new("a.rs")), 0..2);
        // "// ñ😀": 6 unidades UTF-16 (el emoji son dos).
        assert_eq!(spans[0], vec![Span { start: 0, len: 6, highlight: resolve("comment").unwrap() }]);
        assert_eq!(name_at(&spans[1], 8), Some("constant.builtin")); // así marca los enteros la consulta de Rust
    }

    #[test]
    fn edits_reparse_incrementally_and_match_a_fresh_parse() {
        let mut doc = Document::new("fn a() {}\nlet x = 1;\n", "\n");
        let cache = SyntaxCache::default();
        let path = Some(Path::new("a.rs"));
        cache.line_spans(&doc, path, 0..2);
        doc.set_cursor(doc.buffer().len_chars());
        doc.insert("// fin", Instant::now());
        let incremental = cache.line_spans(&doc, path, 0..3);
        let fresh = SyntaxCache::default().line_spans(&doc, path, 0..3);
        assert_eq!(incremental, fresh);
        assert_eq!(name_at(&incremental[2], 0), Some("comment"));
    }

    #[test]
    fn diff_edit_isolates_the_changed_bytes() {
        let e = diff_edit("ab\ncd", "ab\nXcd");
        assert_eq!((e.start_byte, e.old_end_byte, e.new_end_byte), (3, 3, 4));
        assert_eq!(e.start_position, Point { row: 1, column: 0 });
        let e = diff_edit("aña", "aa");
        assert_eq!((e.start_byte, e.old_end_byte, e.new_end_byte), (1, 3, 1));
    }
}
