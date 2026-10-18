//! POSIX `shlex.split(..., comments=False)` and `shlex.quote`.
//!
//! `split` follows Python 3's lexer with `posix=True`, `comments=False` and
//! `whitespace_split=True`. Unbalanced quotes return an error so callers can
//! fall back the way `Generic.split_command` does.

/// Shell-escape `s` the way `shlex.quote` does.
pub fn quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    let unsafe_char = s.chars().any(|c| !is_safe(c));
    if !unsafe_char {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\"'\"'");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

fn is_safe(c: char) -> bool {
    // re.ASCII \w plus @%+=:,./-
    c.is_ascii_alphanumeric() || c == '_' || matches!(c, '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
}

/// Split `input` into shell words. `Err` means an unmatched quote or a
/// trailing escape, which Python raises as `ValueError`.
pub fn split(input: &str) -> Result<Vec<String>, ()> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;
    let mut tokens = Vec::new();
    loop {
        match read_token(&chars, &mut i)? {
            None => break,
            Some(tok) => tokens.push(tok),
        }
    }
    Ok(tokens)
}

fn next_char(chars: &[char], i: &mut usize) -> Option<char> {
    if *i < chars.len() {
        let ch = chars[*i];
        *i += 1;
        Some(ch)
    } else {
        None
    }
}

fn read_token(chars: &[char], i: &mut usize) -> Result<Option<String>, ()> {
    let mut state = ' ';
    let mut escapedstate = ' ';
    let mut token = String::new();
    let mut quoted = false;

    loop {
        let next = next_char(chars, i);
        if state == ' ' {
            match next {
                None => return Ok(None),
                Some(ch) if is_ws(ch) => continue,
                Some('\\') => {
                    escapedstate = 'a';
                    state = '\\';
                }
                Some(ch @ '\'') | Some(ch @ '"') => {
                    state = ch;
                }
                Some(ch) => {
                    token.push(ch);
                    state = 'a';
                }
            }
        } else if state == '\'' || state == '"' {
            quoted = true;
            match next {
                None => return Err(()),
                Some(ch) if ch == state => {
                    state = 'a';
                }
                Some('\\') if state == '"' => {
                    escapedstate = state;
                    state = '\\';
                }
                Some(ch) => token.push(ch),
            }
        } else if state == '\\' {
            match next {
                None => return Err(()),
                Some(ch) => {
                    // Inside quotes, only the quote and the backslash are escapable.
                    if (escapedstate == '\'' || escapedstate == '"') && ch != '\\' && ch != escapedstate {
                        token.push('\\');
                    }
                    token.push(ch);
                    state = escapedstate;
                }
            }
        } else if state == 'a' {
            match next {
                None => break,
                Some(ch) if is_ws(ch) => break,
                Some(ch @ '\'') | Some(ch @ '"') => {
                    state = ch;
                }
                Some('\\') => {
                    escapedstate = 'a';
                    state = '\\';
                }
                Some(ch) => token.push(ch),
            }
        }
    }

    if !quoted && token.is_empty() {
        return Ok(None);
    }
    Ok(Some(token))
}

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// `Generic.split_command`: protect `\ ` from shlex, then restore it.
pub fn split_command(command: &str) -> Vec<String> {
    let protected = command.replace("\\ ", "??");
    match split(&protected) {
        Ok(parts) => parts.into_iter().map(|s| s.replace("??", "\\ ")).collect(),
        Err(()) => command.split(' ').map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python_split(input: &str) -> Result<Vec<String>, ()> {
        let code = format!(
            "import shlex, json, sys\ntry:\n print(json.dumps(shlex.split({input:?})))\nexcept ValueError:\n sys.exit(2)\n"
        );
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg(code)
            .output()
            .unwrap();
        if out.status.code() == Some(2) {
            return Err(());
        }
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        Ok(serde_json::from_slice(&out.stdout).unwrap())
    }

    fn python_quote(input: &str) -> String {
        let code = format!("import shlex; print(shlex.quote({input:?}))");
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg(code)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string()
    }

    #[test]
    fn split_matches_python() {
        let samples = [
            "",
            "git",
            "git status",
            "git commit -m \"new commit\"",
            "echo 'a b'",
            "echo a\\ b",
            "echo \"a\\\"b\"",
            "echo \"\"",
            "a  b",
            "echo # foo",
            "sudo apt-get install vim",
            "cd foo/bar/baz",
            "echo 'unterminated",
            "mkdir && touch a",
            "git push --set-upstream origin master",
            "ls -la 'file with spaces'",
            "echo $HOME",
            "cmd --flag=value",
            "a\\ b c",
            "echo \"foo\\nbar\"",
            "rm -rf /",
            "grep -lir . test",
            "'quoted'",
            "\"double\"",
            "mix 'a' \"b c\" d",
            "trail\\",
            "café latte",
            "git commit -m 'it'\\''s fine'",
        ];
        for sample in samples {
            assert_eq!(split(sample), python_split(sample), "split {sample:?}");
            assert_eq!(quote(sample), python_quote(sample), "quote {sample:?}");
        }
    }

    #[test]
    fn split_command_matches_python_shell() {
        let code = r#"
import json, sys
sys.path.insert(0, ".")
from revise.shells.generic import Generic
samples = sys.stdin.read().split("\0")
print(json.dumps([Generic().split_command(s) for s in samples if s != ""]))
"#;
        let samples = ["echo a\\ b", "git commit -m \"new commit\"", "unterminated \"quote"];
        let mut child = std::process::Command::new("python3")
            .arg("-c")
            .arg(code)
            .current_dir("..")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().unwrap();
            for s in samples {
                stdin.write_all(s.as_bytes()).unwrap();
                stdin.write_all(&[0]).unwrap();
            }
        }
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let expect: Vec<Vec<String>> = serde_json::from_slice(&out.stdout).unwrap();
        for (sample, exp) in samples.iter().zip(expect) {
            assert_eq!(split_command(sample), exp, "{sample:?}");
        }
    }
}
