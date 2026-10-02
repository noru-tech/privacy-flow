//! Fides data maps, as YAML or JSON, parse or return an error. They never panic.
//!
//! The first byte picks the format; the rest is the document.

#![no_main]

use libfuzzer_sys::fuzz_target;
use privacy_flow::datamap::Datamap;

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    let path = if pick % 2 == 0 {
        "datamap.yml"
    } else {
        "datamap.json"
    };
    let _ = Datamap::parse(path, text);
});
