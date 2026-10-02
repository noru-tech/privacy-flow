//! `.privacy-flow.yml`: parsing, schema validation and applying the catalogue extensions and
//! field classifications either succeed or return an error. They never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use privacy_flow::analyze;
use privacy_flow::catalogue::Catalogue;
use privacy_flow::config::Config;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(config) = Config::parse(text, ".privacy-flow.yml") else {
        return;
    };
    let Ok(catalogue) = Catalogue::load(&config.extensions()) else {
        return;
    };
    let _ = analyze::classifier_for(&config, &catalogue);
});
