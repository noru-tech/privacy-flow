//! Which files are analysed, and how their contents are read.
//!
//! Inside a Git work tree the tracked files are enumerated with `git ls-files`; outside one,
//! by a sorted directory walk. Which method was used is recorded in the output, because the
//! two can disagree (an untracked file is scanned by one and not the other). `diff` reads a
//! revision's tree from the object database without checking it out.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::glob::Glob;
use crate::ir::Lang;

/// Files larger than this are not analysed (minified bundles, generated code) and are reported.
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

pub const DEFAULT_EXCLUDES: &[&str] = &[
    // Dependencies, vendored and generated code.
    "**/node_modules/**",
    "**/vendor/**",
    "**/third_party/**",
    "**/bower_components/**",
    "**/dist/**",
    "**/build/**",
    "**/out/**",
    "**/.next/**",
    "**/.nuxt/**",
    "**/.svelte-kit/**",
    "**/coverage/**",
    "**/__pycache__/**",
    "**/.venv/**",
    "**/venv/**",
    "**/site-packages/**",
    "**/.tox/**",
    "**/migrations/**",
    "**/*.min.js",
    "**/*.bundle.js",
    "**/*.d.ts",
    "**/*.generated.*",
    // Tests and fixtures.
    "**/test/**",
    "**/tests/**",
    "**/__tests__/**",
    "**/spec/**",
    "**/*.test.*",
    "**/*.spec.*",
    "**/test_*.py",
    "**/*_test.py",
    "**/conftest.py",
    "**/fixtures/**",
    "**/__fixtures__/**",
    "**/__mocks__/**",
    "**/testdata/**",
    "**/e2e/**",
    "**/cypress/**",
    "**/playwright/**",
    "**/*.stories.*",
    "**/storybook-static/**",
    // Seed data and mocks.
    "**/seed/**",
    "**/seeds/**",
    "**/seeders/**",
    "**/*.seed.*",
    "**/mocks/**",
    "**/__generated__/**",
];

/// Extensions of languages that carry application logic but are not analysed in v0.1.
const UNSUPPORTED: &[(&str, &str)] = &[
    ("go", "go"),
    ("java", "java"),
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("scala", "scala"),
    ("rb", "ruby"),
    ("php", "php"),
    ("cs", "csharp"),
    ("fs", "fsharp"),
    ("rs", "rust"),
    ("swift", "swift"),
    ("m", "objective-c"),
    ("ex", "elixir"),
    ("exs", "elixir"),
    ("erl", "erlang"),
    ("clj", "clojure"),
    ("dart", "dart"),
    ("vue", "vue"),
    ("svelte", "svelte"),
    ("astro", "astro"),
];

pub fn language_of(path: &str) -> Option<Lang> {
    let ext = path.rsplit_once('.').map(|(_, e)| e)?;
    match ext {
        "ts" | "tsx" | "mts" | "cts" => Some(Lang::Typescript),
        "js" | "jsx" | "mjs" | "cjs" => Some(Lang::Javascript),
        "py" | "pyi" if !path.ends_with(".pyi") => Some(Lang::Python),
        _ => None,
    }
}

pub fn unsupported_language_of(path: &str) -> Option<&'static str> {
    let ext = path.rsplit_once('.').map(|(_, e)| e)?;
    UNSUPPORTED.iter().find(|(e, _)| *e == ext).map(|(_, l)| *l)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Git,
    Walk,
    GitRevision,
}

#[derive(Clone, Debug)]
pub struct Listing {
    pub method: Method,
    /// Root-relative, `/`-separated, sorted.
    pub files: Vec<String>,
    pub commit: Option<String>,
    pub dirty: Option<bool>,
}

pub struct Excludes {
    globs: Vec<Glob>,
}

impl Excludes {
    pub fn new(user: &[String], defaults: bool) -> Result<Excludes> {
        let mut globs = Vec::new();
        if defaults {
            for p in DEFAULT_EXCLUDES {
                globs.push(Glob::path(p)?);
            }
        }
        for p in user {
            globs.push(Glob::path(p)?);
        }
        Ok(Excludes { globs })
    }

    pub fn excluded(&self, path: &str) -> bool {
        self.globs.iter().any(|g| g.is_match(path))
    }
}

fn git(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .output()
        .context("running git")
}

pub fn in_work_tree(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"])
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false)
}

/// The scan root's path inside its repository (`""` at the top, `sub/dir/` below it).
pub fn repo_prefix(root: &Path) -> Result<String> {
    let o = git(root, &["rev-parse", "--show-prefix"])?;
    if !o.status.success() {
        bail!("not inside a Git work tree");
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn head_commit(root: &Path) -> Option<String> {
    let o = git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn resolve_rev(root: &Path, rev: &str) -> Result<String> {
    if rev.starts_with('-') {
        bail!("revision {rev:?} must not start with '-'");
    }
    let o = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )?;
    if !o.status.success() {
        bail!("{rev:?} is not a commit in this repository");
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn list(root: &Path) -> Result<Listing> {
    if in_work_tree(root) {
        let o = git(root, &["ls-files", "-z", "--cached", "--", "."])?;
        if !o.status.success() {
            bail!(
                "git ls-files failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
        }
        let mut files: Vec<String> = o
            .stdout
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .filter(|p| root.join(p).is_file())
            .collect();
        files.sort();
        files.dedup();
        let commit = head_commit(root);
        let dirty = git(
            root,
            &["status", "--porcelain", "--untracked-files=no", "--", "."],
        )
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty());
        return Ok(Listing {
            method: Method::Git,
            files,
            commit,
            dirty,
        });
    }
    walk(root)
}

/// Enumerate by a sorted directory walk (skipping `.git`), whatever the root is inside.
pub fn walk(root: &Path) -> Result<Listing> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .follow_links(false)
    {
        let entry = entry.context("walking the scan root")?;
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.split('/').any(|seg| seg == ".git") {
            continue;
        }
        files.push(rel);
    }
    files.sort();
    Ok(Listing {
        method: Method::Walk,
        files,
        commit: None,
        dirty: None,
    })
}

/// The files of `rev` under the scan root, with their contents.
/// (path, bytes) for every file of a revision.
pub type Blobs = Vec<(String, Vec<u8>)>;

pub fn read_revision(root: &Path, rev: &str) -> Result<(Listing, Blobs)> {
    let commit = resolve_rev(root, rev)?;
    let prefix = repo_prefix(root)?;
    let o = git(
        root,
        &["ls-tree", "-r", "-z", "--full-tree", "--name-only", &commit],
    )?;
    if !o.status.success() {
        bail!("git ls-tree failed for {rev}");
    }
    let mut names: Vec<String> = o
        .stdout
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .filter_map(|p| p.strip_prefix(&prefix).map(str::to_string))
        .collect();
    names.sort();
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "--batch"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("running git cat-file")?;
    let mut stdin = child.stdin.take().expect("piped");
    let request: Vec<String> = names
        .iter()
        .map(|n| format!("{commit}:{prefix}{n}\n"))
        .collect();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for line in request {
            stdin.write_all(line.as_bytes())?;
        }
        Ok(())
    });
    let mut out = BufReader::new(child.stdout.take().expect("piped"));
    let mut contents = Vec::with_capacity(names.len());
    for name in &names {
        let mut header = String::new();
        out.read_line(&mut header)?;
        let parts: Vec<&str> = header.trim_end().split(' ').collect();
        if parts.len() != 3 {
            bail!("unexpected git cat-file output for {name}");
        }
        let size: usize = parts[2].parse().context("blob size")?;
        let mut buf = vec![0u8; size];
        out.read_exact(&mut buf)?;
        let mut nl = [0u8; 1];
        out.read_exact(&mut nl)?;
        if parts[1] == "blob" {
            contents.push((name.clone(), buf));
        }
    }
    writer
        .join()
        .expect("writer thread")
        .context("writing to git cat-file")?;
    let status = child.wait()?;
    if !status.success() {
        bail!("git cat-file failed");
    }
    let files = contents.iter().map(|(n, _)| n.clone()).collect();
    Ok((
        Listing {
            method: Method::GitRevision,
            files,
            commit: Some(commit),
            dirty: None,
        },
        contents,
    ))
}

/// Read a work-tree file relative to the root.
pub fn read(root: &Path, rel: &str) -> Result<Vec<u8>> {
    let p: PathBuf = root.join(rel);
    std::fs::read(&p).with_context(|| format!("reading {rel}"))
}

/// Strip `//` and `/* */` comments and trailing commas from JSONC (tsconfig.json).
pub fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut in_str = false;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
                i += 1;
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            ',' => {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if !matches!(chars.get(j), Some('}') | Some(']')) {
                    out.push(c);
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages() {
        assert_eq!(language_of("a/b.tsx"), Some(Lang::Typescript));
        assert_eq!(language_of("a/b.cjs"), Some(Lang::Javascript));
        assert_eq!(language_of("a.py"), Some(Lang::Python));
        assert_eq!(language_of("a.pyi"), None);
        assert_eq!(unsupported_language_of("main.go"), Some("go"));
        assert_eq!(language_of("README.md"), None);
    }

    #[test]
    fn default_excludes() {
        let e = Excludes::new(&[], true).unwrap();
        assert!(e.excluded("node_modules/x/index.js"));
        assert!(e.excluded("src/__tests__/a.ts"));
        assert!(e.excluded("src/a.test.ts"));
        assert!(e.excluded("tests/test_api.py"));
        assert!(e.excluded("types/global.d.ts"));
        assert!(!e.excluded("src/api/users.ts"));
        assert!(!e.excluded("src/latest/x.ts"));
    }
}
