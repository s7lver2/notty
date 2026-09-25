#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VimCmd {
    Save,
    Quit,
    SaveAndQuit,
    Substitute { pattern: String, replacement: String, global: bool, ignore_case: bool },
    Unknown(String),
}

pub fn parse_vim_cmd(line: &str) -> VimCmd {
    match line {
        "w" => return VimCmd::Save,
        "q" => return VimCmd::Quit,
        "wq" | "x" => return VimCmd::SaveAndQuit,
        _ => {}
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
    fn unknown_command_is_preserved_verbatim() {
        assert_eq!(parse_vim_cmd("nope"), VimCmd::Unknown("nope".to_string()));
    }

    #[test]
    fn malformed_substitute_is_unknown() {
        assert_eq!(parse_vim_cmd("%s/solo-dos-barras"), VimCmd::Unknown("%s/solo-dos-barras".to_string()));
    }
}
