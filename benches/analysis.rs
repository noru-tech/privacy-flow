//! Criterion benchmarks: the whole pipeline on the fixture tree and on a generated service, and
//! the two engines on the same facts (the comparison behind docs/adr/0002-flow-engine.md).
//!
//! Run with `cargo bench`; compare runs with criterion's own baseline flags. CI does not run
//! these (shared runners are too noisy for criterion's statistics); it enforces a coarse budget
//! with `.github/scripts/perf_budget.py` instead.

use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};
use privacy_flow::analyze::{self, EngineKind, Tree};
use privacy_flow::engine::{self, Options};
use privacy_flow::files;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A generated TypeScript service of `modules` modules (about 52 lines each), the same shape as
/// benches/synthetic.py.
fn synthetic(modules: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..modules {
        let prev = (i + modules - 1) % modules;
        let other = (i * 7 + 3) % modules;
        let text = format!(
            "import pino from 'pino';\nimport OpenAI from 'openai';\nimport {{ helper{prev} }} from './m{prev}';\nimport {{ Service{other} }} from './m{other}';\nconst logger = pino();\nconst openai = new OpenAI();\nexport interface Account{i} {{ id: string; email: string; phone_number: string; plan: string }}\nexport function helper{i}(a: Account{i}) {{ return `${{a.plan}}-${{a.id}}`; }}\nexport function contact{i}(a: Account{i}) {{ return `${{a.email}} / ${{a.phone_number}}`; }}\nexport class Service{i} {{\n  constructor(private readonly prefix: string) {{}}\n  record(a: Account{i}) {{ logger.info({{ id: a.id }}); if (a.plan === 'x') {{ logger.debug(contact{i}(a)); }} return helper{prev}(a as never); }}\n  async summarize(a: Account{i}) {{ return openai.chat.completions.create({{ model: 'm', messages: [{{ role: 'user', content: a.plan }}] }}); }}\n}}\nexport function wire{i}(a: Account{i}) {{ new Service{i}('a').record(a); new Service{other}('b').record(a as never); return [a].map((x) => helper{i}(x)); }}\n"
        );
        std::fs::write(dir.path().join(format!("m{i}.ts")), text).unwrap();
    }
    dir
}

fn scan(dir: &Path) {
    let tree = Tree::WorkTree(dir);
    let loaded = analyze::load_config(&tree, None).unwrap();
    let listing = files::walk(dir).unwrap();
    let run = analyze::analyze(&tree, &listing, &loaded, EngineKind::Worklist).unwrap();
    std::hint::black_box(run.document.digest);
}

fn benches(c: &mut Criterion) {
    let fx = fixtures();
    c.bench_function("scan fixture tree", |b| b.iter(|| scan(&fx)));
    let synth = synthetic(400);
    c.bench_function("scan synthetic 400 modules", |b| {
        b.iter(|| scan(synth.path()))
    });

    // The engines alone, on prebuilt facts.
    let tree = Tree::WorkTree(synth.path());
    let loaded = analyze::load_config(&tree, None).unwrap();
    let listing = files::walk(synth.path()).unwrap();
    let run = analyze::analyze(&tree, &listing, &loaded, EngineKind::Worklist).unwrap();
    let opts = Options::default();
    c.bench_function("engine worklist (paths)", |b| {
        b.iter(|| std::hint::black_box(engine::worklist::run(&run.facts, &run.program, opts).len()))
    });
    c.bench_function("engine datalog (reachability)", |b| {
        b.iter(|| std::hint::black_box(engine::datalog::run(&run.facts, &run.program, opts).len()))
    });
}

criterion_group! {
    name = analysis;
    config = Criterion::default().sample_size(10);
    targets = benches
}
criterion_main!(analysis);
