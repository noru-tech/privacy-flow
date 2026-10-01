//! Glob patterns for API paths and file paths.
//!
//! API paths are dotted: `openai:().chat.completions.create`, `logging.getLogger().info`.
//! In an API pattern `*` matches within one segment (no `.`), `**` matches any run of
//! segments, and `{a,b}` is alternation. File patterns use `/` as the separator, with `**/`
//! matching zero or more directories.

use anyhow::{Result, bail};
use regex::Regex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Api,
    Path,
}

#[derive(Clone, Debug)]
pub struct Glob {
    pub pattern: String,
    re: Regex,
}

impl Glob {
    pub fn new(pattern: &str, mode: Mode) -> Result<Glob> {
        let mut out = String::from("^");
        let chars: Vec<char> = pattern.chars().collect();
        translate(&chars, &mut 0, mode, &mut out, false)?;
        out.push('$');
        let re = Regex::new(&out)?;
        Ok(Glob {
            pattern: pattern.to_string(),
            re,
        })
    }

    pub fn api(pattern: &str) -> Result<Glob> {
        Glob::new(pattern, Mode::Api)
    }

    pub fn path(pattern: &str) -> Result<Glob> {
        Glob::new(pattern, Mode::Path)
    }

    pub fn is_match(&self, s: &str) -> bool {
        self.re.is_match(s)
    }
}

fn translate(
    chars: &[char],
    i: &mut usize,
    mode: Mode,
    out: &mut String,
    in_brace: bool,
) -> Result<()> {
    let sep = match mode {
        Mode::Api => '.',
        Mode::Path => '/',
    };
    while *i < chars.len() {
        let c = chars[*i];
        match c {
            '*' => {
                if chars.get(*i + 1) == Some(&'*') {
                    *i += 2;
                    if mode == Mode::Path && chars.get(*i) == Some(&'/') {
                        *i += 1;
                        out.push_str("(?:.*/)?");
                    } else {
                        out.push_str(".*");
                    }
                    continue;
                }
                out.push_str(&format!("[^{}]*", regex::escape(&sep.to_string())));
            }
            '?' if mode == Mode::Path => out.push_str("[^/]"),
            '{' => {
                *i += 1;
                out.push_str("(?:");
                translate(chars, i, mode, out, true)?;
                if chars.get(*i) != Some(&'}') {
                    bail!("unclosed '{{' in pattern");
                }
                out.push(')');
            }
            ',' if in_brace => out.push('|'),
            '}' if in_brace => return Ok(()),
            _ => out.push_str(&regex::escape(&c.to_string())),
        }
        *i += 1;
    }
    if in_brace {
        bail!("unclosed '{{' in pattern");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_globs() {
        let g = Glob::api("console.{log,info}").unwrap();
        assert!(g.is_match("console.log"));
        assert!(!g.is_match("console.table"));
        let g = Glob::api("pino:()**.{info,warn}").unwrap();
        assert!(g.is_match("pino:().info"));
        assert!(g.is_match("pino:().child().warn"));
        let g = Glob::api("@sentry/*:setUser").unwrap();
        assert!(g.is_match("@sentry/node:setUser"));
        assert!(!g.is_match("@sentry/node:x.setUser"));
        let g = Glob::api("openai:{(),OpenAI()}.chat.completions.create").unwrap();
        assert!(g.is_match("openai:().chat.completions.create"));
        assert!(g.is_match("openai:OpenAI().chat.completions.create"));
        assert!(Glob::api("a{b").is_err());
    }

    #[test]
    fn path_globs() {
        let g = Glob::path("**/app/**/route.{ts,js}").unwrap();
        assert!(g.is_match("app/api/users/route.ts"));
        assert!(g.is_match("src/app/route.js"));
        assert!(!g.is_match("src/app/route.tsx"));
        let g = Glob::path("**/*.test.*").unwrap();
        assert!(g.is_match("a/b/c.test.ts"));
        assert!(g.is_match("c.test.ts"));
        let g = Glob::path("node_modules/**").unwrap();
        assert!(g.is_match("node_modules/x/y.js"));
    }
}
