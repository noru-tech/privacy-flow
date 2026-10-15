//! Fides data maps, as YAML or JSON, parse or return an error, and Prisma schemas are read into
//! models. They never panic.
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
    match pick % 3 {
        0 => drop(Datamap::parse("datamap.yml", text)),
        1 => drop(Datamap::parse("datamap.json", text)),
        _ => drop(privacy_flow::topology::prisma::parse(text)),
    }
});
