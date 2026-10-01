//! Principle 1: the same files and config produce byte-identical output on every platform and
//! thread count.
//!
//! - Every fixture is analysed with 1 and with 8 worker threads, twice each; all four outputs
//!   must be the same bytes.
//! - Every fixture's canonical JSON is compared with a committed golden. CI runs this on Linux
//!   and macOS, so a platform difference fails the build. Regenerate after an intended change
//!   with `UPDATE_GOLDENS=1 cargo test --test determinism`, and review the diff.
//! - The whole fixture tree is also analysed as one program, the largest input the suite has.

mod common;

use privacy_flow::analyze::EngineKind;
use privacy_flow::output::{self, Format};

fn with_threads<T: Send>(n: usize, f: impl FnOnce() -> T + Send) -> T {
    rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .unwrap()
        .install(f)
}

fn render(dir: &std::path::Path, format: Format) -> String {
    let doc = common::run(dir, EngineKind::Worklist).document;
    output::render(&doc, format).unwrap()
}

#[test]
fn output_is_identical_across_thread_counts_and_runs() {
    let mut dirs = common::fixtures();
    dirs.push(common::fixtures_root());
    for dir in dirs {
        let one = with_threads(1, || render(&dir, Format::Json));
        let one_again = with_threads(1, || render(&dir, Format::Json));
        let many = with_threads(8, || render(&dir, Format::Json));
        let many_again = with_threads(8, || render(&dir, Format::Json));
        assert_eq!(
            one,
            one_again,
            "{}: two single-threaded runs differ",
            dir.display()
        );
        assert_eq!(
            one,
            many,
            "{}: 1 thread and 8 threads differ",
            dir.display()
        );
        assert_eq!(
            many,
            many_again,
            "{}: two 8-thread runs differ",
            dir.display()
        );
    }
}

#[test]
fn output_matches_committed_goldens() {
    let goldens = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    let update = std::env::var_os("UPDATE_GOLDENS").is_some();
    let mut failures = Vec::new();
    for dir in common::fixtures() {
        let name = common::name(&dir).replace('/', "__");
        let mut outputs = vec![(format!("{name}.json"), render(&dir, Format::Json))];
        if name == "ts__pf003-fail" || name == "py__pf004-fail" {
            outputs.push((format!("{name}.sarif"), render(&dir, Format::Sarif)));
            outputs.push((format!("{name}.fides.yml"), render(&dir, Format::Fides)));
            outputs.push((format!("{name}.txt"), render(&dir, Format::Table)));
        }
        for (file, bytes) in outputs {
            let path = goldens.join(&file);
            if update {
                std::fs::create_dir_all(&goldens).unwrap();
                std::fs::write(&path, &bytes).unwrap();
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(want) if want == bytes => {}
                Ok(_) => failures.push(format!("{file}: differs from the golden")),
                Err(_) => failures.push(format!("{file}: no golden (run with UPDATE_GOLDENS=1)")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
