#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VimCmd {
    /// `:w` (sin ruta: como Ctrl+S, que pregunta dónde si el documento no tiene).
    Save,
    /// `:w ruta`: guardar como.
    SaveAs(String),
    /// `:wa`: guarda todos los que tienen ruta.
    SaveAll,
    /// `:q`: cierra la pestaña (pregunta si hay cambios).
    Quit,
    /// `:q!`: cierra la pestaña descartando los cambios.
    ForceQuit,
    /// `:wq`, `:x`.
    SaveAndQuit,
    /// `:qa`: cierra notty (pregunta por lo no guardado).
    QuitAll,
    /// `:qa!`: cierra notty sin preguntar.
    ForceQuitAll,
    /// `:wqa`, `:xa`.
    SaveAndQuitAll,
    /// `:e ruta`, `:tabnew ruta`.
    Edit(String),
    /// `:e!`: vuelve a cargar el archivo del disco, descartando los cambios.
    Revert,
    /// `:enew`, `:tabnew`.
    New,
    /// `:bn`, `:tabn`.
    NextTab,
    /// `:bp`, `:tabp`.
    PrevTab,
    /// `:12` (líneas desde 1) o `:$` (`usize::MAX` = la última).
    GotoLine(usize),
    /// `:set nu` / `:set nonu` / `:set nu!`: `Some(on)` o `None` para alternar.
    LineNumbers(Option<bool>),
    Substitute { pattern: String, replacement: String, global: bool, ignore_case: bool },
    Unknown(String),
}

pub fn parse_vim_cmd(line: &str) -> VimCmd {
    let line = line.trim();
    let (cmd, arg) = match line.split_once(char::is_whitespace) {
        Some((c, a)) => (c, a.trim()),
        None => (line, ""),
    };
    let parsed = match (cmd, arg) {
        ("w" | "write", "") => Some(VimCmd::Save),
        ("w" | "write" | "saveas", path) if !path.is_empty() => Some(VimCmd::SaveAs(path.to_string())),
        ("wa" | "wall", "") => Some(VimCmd::SaveAll),
        ("q" | "quit" | "clo" | "close", "") => Some(VimCmd::Quit),
        ("q!" | "quit!" | "clo!" | "close!", "") => Some(VimCmd::ForceQuit),
        ("wq" | "x" | "xit" | "exit", "") => Some(VimCmd::SaveAndQuit),
        ("qa" | "qall" | "quitall", "") => Some(VimCmd::QuitAll),
        ("qa!" | "qall!" | "quitall!", "") => Some(VimCmd::ForceQuitAll),
        ("wqa" | "wqall" | "xa" | "xall", "") => Some(VimCmd::SaveAndQuitAll),
        ("e" | "edit" | "tabe" | "tabedit" | "tabnew", path) if !path.is_empty() => Some(VimCmd::Edit(path.to_string())),
        ("e!" | "edit!", "") => Some(VimCmd::Revert),
        ("enew" | "tabnew" | "new", "") => Some(VimCmd::New),
        ("bn" | "bnext" | "tabn" | "tabnext", "") => Some(VimCmd::NextTab),
        ("bp" | "bprev" | "bprevious" | "bN" | "tabp" | "tabprev" | "tabprevious" | "tabN", "") => Some(VimCmd::PrevTab),
        ("$", "") => Some(VimCmd::GotoLine(usize::MAX)),
        ("set" | "se", "nu" | "number") => Some(VimCmd::LineNumbers(Some(true))),
        ("set" | "se", "nonu" | "nonumber") => Some(VimCmd::LineNumbers(Some(false))),
        ("set" | "se", "nu!" | "number!" | "invnu" | "invnumber") => Some(VimCmd::LineNumbers(None)),
        (n, "") if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => n.parse().ok().map(VimCmd::GotoLine),
        _ => None,
    };
    if let Some(cmd) = parsed {
        return cmd;
    }
    if let Some(rest) = line.strip_prefix("%s/") {
        if let Some(cmd) = parse_substitute(rest) {
            return cmd;
        }
    }
    VimCmd::Unknown(line.to_string())
}

fn parse_substitute(rest: &str) -> Option<VimCmd> {
    let parts = split_unescaped_slash(rest);
    if parts.len() != 3 {
        return None;
    }
    let flags = parts[2];
    Some(VimCmd::Substitute {
        pattern: parts[0].replace(r"\/", "/"),
        replacement: parts[1].replace(r"\/", "/"),
        global: flags.contains('g'),
        ignore_case: flags.contains('i'),
    })
}

fn split_unescaped_slash(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && (i == 0 || bytes[i - 1] != b'\\') {
            out.push(&s[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w_q_wq() {
        assert_eq!(parse_vim_cmd("w"), VimCmd::Save);
        assert_eq!(parse_vim_cmd("q"), VimCmd::Quit);
        assert_eq!(parse_vim_cmd("wq"), VimCmd::SaveAndQuit);
        assert_eq!(parse_vim_cmd("x"), VimCmd::SaveAndQuit);
    }

    #[test]
    fn substitute_without_flags() {
        assert_eq!(
            parse_vim_cmd("%s/hola/adios/"),
            VimCmd::Substitute { pattern: "hola".into(), replacement: "adios".into(), global: false, ignore_case: false }
        );
    }

    #[test]
    fn substitute_with_global_and_case_flags() {
        assert_eq!(
            parse_vim_cmd("%s/a/b/gi"),
            VimCmd::Substitute { pattern: "a".into(), replacement: "b".into(), global: true, ignore_case: true }
        );
    }

    #[test]
    fn substitute_allows_escaped_slash() {
        assert_eq!(
            parse_vim_cmd(r"%s/a\/b/c/"),
            VimCmd::Substitute { pattern: "a/b".into(), replacement: "c".into(), global: false, ignore_case: false }
        );
    }

    #[test]
    fn more_commands() {
        assert_eq!(parse_vim_cmd("q!"), VimCmd::ForceQuit);
        assert_eq!(parse_vim_cmd("qa!"), VimCmd::ForceQuitAll);
        assert_eq!(parse_vim_cmd("wqa"), VimCmd::SaveAndQuitAll);
        assert_eq!(parse_vim_cmd("w  C:\\a b.txt "), VimCmd::SaveAs("C:\\a b.txt".into()));
        assert_eq!(parse_vim_cmd("e notas.md"), VimCmd::Edit("notas.md".into()));
        assert_eq!(parse_vim_cmd("e!"), VimCmd::Revert);
        assert_eq!(parse_vim_cmd("tabnew"), VimCmd::New);
        assert_eq!(parse_vim_cmd("42"), VimCmd::GotoLine(42));
        assert_eq!(parse_vim_cmd("$"), VimCmd::GotoLine(usize::MAX));
        assert_eq!(parse_vim_cmd("set nonu"), VimCmd::LineNumbers(Some(false)));
        assert_eq!(parse_vim_cmd("bp"), VimCmd::PrevTab);
    }

    #[test]
    fn unknown_command_is_preserved_verbatim() {
        assert_eq!(parse_vim_cmd("nope"), VimCmd::Unknown("nope".to_string()));
    }

    #[test]
    fn malformed_substitute_is_unknown() {
        assert_eq!(parse_vim_cmd("%s/solo-dos-barras"), VimCmd::Unknown("%s/solo-dos-barras".to_string()));
    }
}
