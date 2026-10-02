//! Lowering one file never panics, whatever the bytes: a file that does not parse is lowered as
//! far as the tree goes and carries a parse-error note.
//!
//! The first byte picks the language (and, for TypeScript, the TSX grammar); the rest is the
//! source, decoded the way `analyze` decodes it.

#![no_main]

use libfuzzer_sys::fuzz_target;
use privacy_flow::{files, lower};

const PATHS: [&str; 4] = ["a.ts", "a.tsx", "a.js", "a.py"];

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    let path = PATHS[usize::from(pick) % PATHS.len()];
    let lang = files::language_of(path).expect("a supported extension");
    let source = String::from_utf8_lossy(rest);
    lower::lower_file(path, lang, &source);
});
