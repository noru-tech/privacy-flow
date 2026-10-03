//! The flow-facts document: what `scan` writes and every other command reads.
//!
//! The document separates **facts** (sources, sinks, flows with their cited paths, coverage
//! gaps) from **judgements derived from them** (findings, egress, summary). The derivation is a
//! pure function of the facts, the embedded policy and the declared processors, so `validate`
//! recomputes it and rejects a document whose findings do not follow. Dispositions are the one
//! part a human edits; they are excluded from the digest.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::catalogue::{Catalogue, SinkClass};
use crate::classify::{Classifier, UNKNOWN};
use crate::config::{Config, ProcessorDecl, Severity, SystemDecl};
use crate::engine::{Reach, Step, Target};
use crate::facts::{Facts, GapKind, HitKind, Host, StaticGap};
use crate::files::Method;
use crate::glob::Glob;
use crate::program::{GKind, Program};
use crate::rules::{RULES, rule};

pub const SCHEMA_ID: &str =
    "https://github.com/noru-tech/privacy-flow/blob/main/schemas/flows.schema.json";
pub const SCHEMA_VERSION: &str = "0.1";
pub const TOOL: &str = "privacy-flow";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Loc {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct GapLoc {
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CatalogueRef {
    pub version: String,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigRef {
    pub path: Option<String>,
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LanguageCount {
    pub language: String,
    pub files: u32,
    pub lines: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    pub enumerated_by: Method,
    pub commit: Option<String>,
    pub dirty: Option<bool>,
    pub files: u32,
    pub languages: Vec<LanguageCount>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Analysis {
    pub engine: String,
    pub max_call_depth: u32,
    pub datamaps: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuleSetting {
    pub enabled: bool,
    pub severity: Severity,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub fail_on: Severity,
    pub rules: BTreeMap<String, RuleSetting>,
}

impl Policy {
    pub fn from_config(c: &Config) -> Policy {
        let mut rules = BTreeMap::new();
        for r in RULES {
            let o = c.policy.rules.get(r.id);
            rules.insert(
                r.id.to_string(),
                RuleSetting {
                    enabled: o.is_none_or(|o| o.enabled),
                    severity: o.and_then(|o| o.severity).unwrap_or(r.severity),
                },
            );
        }
        Policy {
            fail_on: c.policy.fail_on.unwrap_or(Severity::Medium),
            rules,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceRec {
    pub id: String,
    pub kind: String,
    pub category: String,
    pub needs_review: bool,
    pub name: String,
    pub classified_by: String,
    pub function: String,
    pub location: Loc,
    pub text: String,
    pub declared_at: Option<Loc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct HostRec {
    pub kind: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SinkRec {
    pub id: String,
    pub catalogue_id: String,
    pub class: SinkClass,
    pub processor: Option<String>,
    pub heuristic: bool,
    pub host: Option<HostRec>,
    pub function: String,
    pub location: Loc,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Hop {
    pub kind: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub text: String,
    /// The field a `read` or `write` hop reads or writes.
    pub field: Option<String>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FlowRec {
    pub id: String,
    pub source: String,
    pub sink: String,
    pub category: String,
    pub call_depth: u32,
    pub path: Vec<Hop>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GapRec {
    pub id: String,
    pub kind: String,
    pub detail: String,
    pub locations: Vec<GapLoc>,
    /// Sources whose data reached the gap (empty for gaps that exist whatever the data).
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub complete: bool,
    pub gaps: Vec<GapRec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EgressRec {
    pub processor: Option<String>,
    pub fides_key: Option<String>,
    pub class: SinkClass,
    pub host: Option<HostRec>,
    pub categories: Vec<String>,
    pub declared: bool,
    pub citation: Option<String>,
    pub flows: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DispositionStatus {
    Open,
    Accepted,
    Remediated,
    FalsePositive,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Disposition {
    pub status: DispositionStatus,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub decided_at: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub remediated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FindingRec {
    pub id: String,
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub flow: Option<String>,
    pub gap: Option<String>,
    pub categories: Vec<String>,
    pub processor: Option<String>,
    pub needs_review: bool,
    pub heuristic: bool,
    pub location: Option<Loc>,
    pub disposition: Option<Disposition>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub sources: u32,
    pub sinks: u32,
    pub flows: u32,
    pub gaps: u32,
    pub findings: BTreeMap<String, u32>,
    pub by_severity: BTreeMap<String, u32>,
    pub processors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DiffInfo {
    pub base: String,
    pub head: String,
    pub introduced: u32,
    pub changed: u32,
    pub removed: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CitedFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Document {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub schema_version: String,
    pub tool: Tool,
    pub catalogue: CatalogueRef,
    pub config: ConfigRef,
    pub subject: Subject,
    pub analysis: Analysis,
    pub policy: Policy,
    pub system: Option<SystemDecl>,
    pub processors: Vec<ProcessorDecl>,
    pub sources: Vec<SourceRec>,
    pub sinks: Vec<SinkRec>,
    pub flows: Vec<FlowRec>,
    pub coverage: Coverage,
    pub egress: Vec<EgressRec>,
    pub findings: Vec<FindingRec>,
    pub summary: Summary,
    pub cited_files: Vec<CitedFile>,
    pub diff: Option<DiffInfo>,
    pub digest: String,
}

/// First 16 hex digits of SHA-256 over the RFC 8785 serialization of `parts`.
pub fn short_hash<T: Serialize>(parts: &T) -> String {
    let d = canonical::digest(parts).expect("serializable");
    d["sha256:".len().."sha256:".len() + 16].to_string()
}

/// Everything the report needs besides the program and facts.
pub struct Context<'a> {
    pub catalogue: &'a Catalogue,
    pub classifier: &'a Classifier,
    pub config: &'a Config,
    pub config_ref: ConfigRef,
    pub subject: Subject,
    pub analysis: Analysis,
    /// path → sha256 of the bytes analysed, for every analysed file.
    pub file_digests: &'a BTreeMap<String, String>,
}

struct Anchors<'a> {
    p: &'a Program,
}

impl<'a> Anchors<'a> {
    fn loc_of_stmt(&self, stmt: u32) -> Loc {
        let s = &self.p.stmts[stmt as usize];
        Loc {
            path: self.p.files[s.file as usize].path.clone(),
            line: s.pos.line,
            column: s.pos.column,
        }
    }

    fn func_name_of_stmt(&self, stmt: u32) -> String {
        let s = &self.p.stmts[stmt as usize];
        self.p.funcs[s.func as usize].name.clone()
    }
}

/// Assign IDs from anchors that survive unrelated edits (no line numbers); duplicates of the
/// same anchor are numbered in source order. The same anchor at the same place is one source
/// or sink, and one ID: a base method copied into its subclasses has its sinks once.
fn ids_from_anchors(prefix: &str, anchors: &[(String, (String, u32, u32))]) -> Vec<String> {
    use sha2::{Digest, Sha256};
    let mut order: Vec<usize> = (0..anchors.len()).collect();
    order.sort_by(|&a, &b| anchors[a].1.cmp(&anchors[b].1).then(a.cmp(&b)));
    let mut seen: HashMap<&str, u32> = HashMap::new();
    let mut placed: HashMap<(&str, &(String, u32, u32)), String> = HashMap::new();
    let mut out = vec![String::new(); anchors.len()];
    for i in order {
        if let Some(id) = placed.get(&(anchors[i].0.as_str(), &anchors[i].1)) {
            out[i] = id.clone();
            continue;
        }
        let n = seen.entry(anchors[i].0.as_str()).or_insert(0);
        let mut h = Sha256::new();
        h.update(anchors[i].0.as_bytes());
        h.update([0x1f]);
        h.update(n.to_string().as_bytes());
        let d = h.finalize();
        let hex: String = d.iter().take(8).map(|b| format!("{b:02x}")).collect();
        out[i] = format!("{prefix}-{hex}");
        placed.insert((anchors[i].0.as_str(), &anchors[i].1), out[i].clone());
        *n += 1;
    }
    out
}

/// An anchor: the parts that identify a source or sink without its line, joined by U+001F.
fn anchor_key(parts: &[&str]) -> String {
    parts.join("\u{1f}")
}

pub fn build(p: &Program, f: &Facts, reaches: &[Reach], ctx: &Context) -> Result<Document> {
    let a = Anchors { p };
    let cat = ctx.catalogue;

    // ---------------------------------------------------------------- sources (all seeds)
    let seed_anchors: Vec<(String, (String, u32, u32))> = f
        .seeds
        .iter()
        .map(|s| {
            let path = &p.files[s.file as usize].path;
            let func = &p.funcs[p.vars[s.var as usize].func as usize].name;
            (
                anchor_key(&[path, func, s.kind.as_str(), &s.name, &s.category, &s.text]),
                (path.clone(), s.pos.line, s.pos.column),
            )
        })
        .collect();
    let seed_ids = ids_from_anchors("src", &seed_anchors);

    // ---------------------------------------------------------------- hits
    let hit_anchors: Vec<(String, (String, u32, u32))> = f
        .hits
        .iter()
        .map(|h| {
            let loc = a.loc_of_stmt(h.stmt);
            let what = match &h.kind {
                HitKind::Sink { sink, .. } => cat.sinks[*sink as usize].def.id.clone(),
                HitKind::Gap { kind, detail } => format!("{}:{detail}", kind.as_str()),
            };
            let text = &p.stmts[h.stmt as usize].text;
            let func = a.func_name_of_stmt(h.stmt);
            (
                anchor_key(&[&loc.path, &func, &what, text]),
                (loc.path, loc.line, loc.column),
            )
        })
        .collect();
    let hit_ids = ids_from_anchors("snk", &hit_anchors);

    // ---------------------------------------------------------------- flows and reached gaps
    struct RawFlow {
        seed: u32,
        hit: u32,
        depth: u32,
        path: Vec<Step>,
    }
    let mut raw_flows: Vec<RawFlow> = Vec::new();
    // gap key → (kind, detail, locations, source seeds)
    let mut reached: BTreeMap<(GapKind, String), (BTreeSet<GapLoc>, BTreeSet<u32>)> =
        BTreeMap::new();
    for r in reaches {
        match r.target {
            Target::Hit(h) => match &f.hits[h as usize].kind {
                HitKind::Sink { .. } => raw_flows.push(RawFlow {
                    seed: r.seed,
                    hit: h,
                    depth: r.depth,
                    path: r.path.clone(),
                }),
                HitKind::Gap { kind, detail } => {
                    let loc = a.loc_of_stmt(f.hits[h as usize].stmt);
                    let e = reached
                        .entry((*kind, normalize_gap_detail(detail)))
                        .or_default();
                    e.0.insert(GapLoc {
                        path: loc.path,
                        line: Some(loc.line),
                        column: Some(loc.column),
                    });
                    e.1.insert(r.seed);
                }
            },
            Target::DepthBound(site) => {
                let stmt = f.sites[site as usize].stmt;
                let loc = a.loc_of_stmt(stmt);
                let callee = p.funcs[f.sites[site as usize].func as usize].name.clone();
                let detail = format!("call to {callee} in {}", a.func_name_of_stmt(stmt));
                let e = reached.entry((GapKind::DepthBound, detail)).or_default();
                e.0.insert(GapLoc {
                    path: loc.path,
                    line: Some(loc.line),
                    column: Some(loc.column),
                });
                e.1.insert(r.seed);
            }
        }
    }
    // One datum, several sources: a typed parameter and a read of that field further down the
    // same path, or a parameter named `email` and the caller's own source passed into it. A flow
    // whose source is a hop of a longer flow to the same sink and category adds nothing; the
    // longer chain already cites it.
    let anchor = |seed: u32| -> Option<Step> {
        let s = &f.seeds[seed as usize];
        s.stmt.map(Step::Edge)
    };
    let enters = |rf: &RawFlow, func: u32, formal: u32| {
        rf.path.iter().any(|st| matches!(st, Step::Enter { site, formal: j } if f.sites[*site as usize].func == func && *j == formal))
    };
    // Flows only subsume flows to the same sink: work per sink.
    let mut by_hit: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, rf) in raw_flows.iter().enumerate() {
        by_hit.entry(rf.hit).or_default().push(i);
    }
    let mut kept_idx: Vec<usize> = Vec::new();
    for group in by_hit.values() {
        let mut order = group.clone();
        order.sort_by(|&x, &y| {
            raw_flows[y]
                .path
                .len()
                .cmp(&raw_flows[x].path.len())
                .then(x.cmp(&y))
        });
        let mut kept: Vec<usize> = Vec::new();
        for &i in &order {
            let rf = &raw_flows[i];
            let s = &f.seeds[rf.seed as usize];
            let subsumed = kept.iter().any(|&k| {
                let other = &raw_flows[k];
                if other.seed == rf.seed || f.seeds[other.seed as usize].category != s.category {
                    return false;
                }
                let by_stmt = anchor(rf.seed).is_some_and(|a| other.path.contains(&a));
                let by_param = s
                    .param
                    .is_some_and(|(func, formal)| enters(other, func, formal));
                by_stmt || by_param
            });
            // Request input of unknown category narrowed by a classified field read on its way:
            // `request.form["api_key"]` is credentials, not "unknown", once the field is named.
            let narrowed = s.category == UNKNOWN
                && group.iter().any(|&o| {
                    let other = &raw_flows[o];
                    f.seeds[other.seed as usize].category != UNKNOWN
                        && anchor(other.seed).is_some_and(|a| rf.path.contains(&a))
                });
            if !subsumed && !narrowed {
                kept.push(i);
            }
        }
        kept_idx.extend(kept);
    }
    kept_idx.sort();
    let raw_flows: Vec<RawFlow> = kept_idx
        .into_iter()
        .map(|i| RawFlow {
            seed: raw_flows[i].seed,
            hit: raw_flows[i].hit,
            depth: raw_flows[i].depth,
            path: raw_flows[i].path.clone(),
        })
        .collect();

    let mut used_seeds = BTreeSet::new();
    let mut used_hits = BTreeSet::new();
    let mut flows = Vec::new();
    let mut flow_ids: BTreeSet<String> = BTreeSet::new();
    let mut cited: BTreeSet<String> = BTreeSet::new();
    for rf in &raw_flows {
        let s = &f.seeds[rf.seed as usize];
        let source_id = seed_ids[rf.seed as usize].clone();
        let sink_id = hit_ids[rf.hit as usize].clone();
        let id = format!(
            "flow-{}",
            short_hash(&serde_json::json!([source_id, sink_id, s.category]))
        );
        // The same flow through another copy of a base method.
        if !flow_ids.insert(id.clone()) {
            continue;
        }
        used_seeds.insert(rf.seed);
        used_hits.insert(rf.hit);
        let path = hops(p, f, cat, rf.seed, rf.hit, &rf.path);
        for h in &path {
            cited.insert(h.path.clone());
        }
        flows.push(FlowRec {
            id,
            source: source_id,
            sink: sink_id,
            category: s.category.clone(),
            call_depth: rf.depth,
            path,
        });
    }

    let sources: Vec<SourceRec> = used_seeds
        .iter()
        .map(|&i| {
            let s = &f.seeds[i as usize];
            SourceRec {
                id: seed_ids[i as usize].clone(),
                kind: s.kind.as_str().to_string(),
                category: s.category.clone(),
                needs_review: s.needs_review,
                name: s.name.clone(),
                classified_by: s.by.clone(),
                function: p.funcs[p.vars[s.var as usize].func as usize].name.clone(),
                location: Loc {
                    path: p.files[s.file as usize].path.clone(),
                    line: s.pos.line,
                    column: s.pos.column,
                },
                text: s.text.clone(),
                declared_at: s.declared_at.map(|(file, pos)| Loc {
                    path: p.files[file as usize].path.clone(),
                    line: pos.line,
                    column: pos.column,
                }),
            }
        })
        .collect();
    let sinks: Vec<SinkRec> = used_hits
        .iter()
        .map(|&i| {
            let h = &f.hits[i as usize];
            let HitKind::Sink {
                sink,
                heuristic,
                host,
            } = &h.kind
            else {
                unreachable!("flows end at sinks")
            };
            let def = &cat.sinks[*sink as usize].def;
            SinkRec {
                id: hit_ids[i as usize].clone(),
                catalogue_id: def.id.clone(),
                class: def.class,
                processor: def.processor.clone(),
                heuristic: *heuristic,
                host: host_rec(host),
                function: a.func_name_of_stmt(h.stmt),
                location: a.loc_of_stmt(h.stmt),
                text: p.stmts[h.stmt as usize].text.clone(),
            }
        })
        .collect();

    // ---------------------------------------------------------------- coverage
    let mut gaps: Vec<GapRec> = Vec::new();
    for g in &f.static_gaps {
        gaps.push(static_gap(g));
    }
    for ((kind, detail), (locs, seeds)) in reached {
        let mut sources: Vec<String> = seeds
            .iter()
            .map(|&s| seed_ids[s as usize].clone())
            .collect();
        sources.sort();
        gaps.push(GapRec {
            id: format!(
                "gap-{}",
                short_hash(&serde_json::json!([kind.as_str(), detail]))
            ),
            kind: kind.as_str().to_string(),
            detail,
            locations: locs.into_iter().collect(),
            sources,
        });
    }
    gaps.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(a.detail.cmp(&b.detail))
            .then(a.id.cmp(&b.id))
    });
    gaps.dedup_by(|a, b| a.id == b.id);
    for g in &gaps {
        for l in &g.locations {
            cited.insert(l.path.clone());
        }
    }

    let cited_files = cited
        .into_iter()
        .filter_map(|path| {
            ctx.file_digests.get(&path).map(|d| CitedFile {
                path,
                sha256: d.clone(),
            })
        })
        .collect();

    let mut doc = Document {
        schema: SCHEMA_ID.into(),
        schema_version: SCHEMA_VERSION.into(),
        tool: Tool {
            name: TOOL.into(),
            version: VERSION.into(),
        },
        catalogue: CatalogueRef {
            version: cat.version.clone(),
            digest: cat.digest.clone(),
        },
        config: ctx.config_ref.clone(),
        subject: ctx.subject.clone(),
        analysis: ctx.analysis.clone(),
        policy: Policy::from_config(ctx.config),
        system: ctx.config.system.clone(),
        processors: ctx.config.processors.clone(),
        sources,
        sinks,
        flows,
        coverage: Coverage {
            complete: gaps.is_empty(),
            gaps,
        },
        egress: Vec::new(),
        findings: Vec::new(),
        summary: Summary {
            sources: f.seeds.len() as u32,
            sinks: f
                .hits
                .iter()
                .filter(|h| matches!(h.kind, HitKind::Sink { .. }))
                .count() as u32,
            flows: 0,
            gaps: 0,
            findings: BTreeMap::new(),
            by_severity: BTreeMap::new(),
            processors: Vec::new(),
        },
        cited_files,
        diff: None,
        digest: String::new(),
    };
    sort_facts(&mut doc);
    derive(&mut doc, ctx.classifier)?;
    seal(&mut doc)?;
    Ok(doc)
}

/// One gap per API, not per chain: consecutive repeats collapse (`where().where().where()` →
/// `where()`), and long chains keep their first three and last three segments.
pub fn normalize_gap_detail(detail: &str) -> String {
    let (module, path) = match detail.split_once(':') {
        Some((m, p)) => (Some(m), p),
        None => (None, detail),
    };
    let mut segs: Vec<&str> = Vec::new();
    for seg in path.split('.') {
        if segs.last() != Some(&seg) {
            segs.push(seg);
        }
    }
    let body = if segs.len() > 8 {
        format!(
            "{}.….{}",
            segs[..3].join("."),
            segs[segs.len() - 3..].join(".")
        )
    } else {
        segs.join(".")
    };
    match module {
        Some(m) => format!("{m}:{body}"),
        None => body,
    }
}

fn host_rec(h: &Host) -> Option<HostRec> {
    match h {
        Host::None => None,
        Host::Literal(v) => Some(HostRec {
            kind: "literal".into(),
            value: Some(v.clone()),
        }),
        Host::Relative => Some(HostRec {
            kind: "relative".into(),
            value: None,
        }),
        Host::Dynamic => Some(HostRec {
            kind: "dynamic".into(),
            value: None,
        }),
    }
}

fn static_gap(g: &StaticGap) -> GapRec {
    let key = match g.kind {
        GapKind::UnsupportedLanguage | GapKind::UnsupportedFramework => {
            serde_json::json!([g.kind.as_str(), g.detail.split(':').next().unwrap_or("")])
        }
        _ => serde_json::json!([
            g.kind.as_str(),
            g.locations.first().map(|l| l.0.clone()),
            g.detail
        ]),
    };
    GapRec {
        id: format!("gap-{}", short_hash(&key)),
        kind: g.kind.as_str().to_string(),
        detail: g.detail.clone(),
        locations: g
            .locations
            .iter()
            .map(|(path, pos)| GapLoc {
                path: path.clone(),
                line: pos.map(|p| p.line),
                column: pos.map(|p| p.column),
            })
            .collect(),
        sources: Vec::new(),
    }
}

fn hops(p: &Program, f: &Facts, cat: &Catalogue, seed: u32, hit: u32, steps: &[Step]) -> Vec<Hop> {
    let s = &f.seeds[seed as usize];
    let mut out: Vec<Hop> = Vec::new();
    let note = match &s.declared_at {
        Some((file, pos)) => format!(
            "{} ({}: {}, field declared at {}:{}; classified by {})",
            s.category,
            s.kind.as_str(),
            s.name,
            p.files[*file as usize].path,
            pos.line,
            s.by
        ),
        None => format!(
            "{} ({}: {}; classified by {})",
            s.category,
            s.kind.as_str(),
            s.name,
            s.by
        ),
    };
    out.push(Hop {
        kind: "source".into(),
        path: p.files[s.file as usize].path.clone(),
        line: s.pos.line,
        column: s.pos.column,
        text: s.text.clone(),
        field: None,
        note: Some(note),
    });
    let stmt_hop = |stmt: u32, kind: &str, note: Option<String>| {
        let st = &p.stmts[stmt as usize];
        Hop {
            kind: kind.to_string(),
            path: p.files[st.file as usize].path.clone(),
            line: st.pos.line,
            column: st.pos.column,
            text: st.text.clone(),
            field: None,
            note,
        }
    };
    for step in steps {
        let hop = match step {
            Step::Edge(stmt) => {
                let kind = match &p.stmts[*stmt as usize].kind {
                    GKind::Copy { .. } => "assign",
                    GKind::Load { .. } | GKind::ThisLoad { .. } => "read",
                    GKind::Store { .. } | GKind::ThisStore { .. } => "write",
                    GKind::Concat { .. } => "concat",
                    GKind::Call { .. } => "call",
                    GKind::Return { .. } => "return",
                    _ => "step",
                };
                let mut hop = stmt_hop(*stmt, kind, None);
                hop.field = match &p.stmts[*stmt as usize].kind {
                    GKind::Load { field: Some(k), .. }
                    | GKind::Store { field: Some(k), .. }
                    | GKind::ThisLoad { field: k, .. }
                    | GKind::ThisStore { field: k, .. } => Some(p.syms.str(*k).to_string()),
                    _ => None,
                };
                hop
            }
            Step::Enter { site, formal } => {
                let site = &f.sites[*site as usize];
                let func = &p.funcs[site.func as usize];
                let param = f.formals[site.func as usize]
                    .get(*formal as usize)
                    .map(|v| p.vars[*v as usize].name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| format!("#{formal}"));
                stmt_hop(
                    site.stmt,
                    "call",
                    Some(format!("into {} (parameter {param})", func.name)),
                )
            }
            Step::Exit { site } => {
                let site = &f.sites[*site as usize];
                let func = &p.funcs[site.func as usize];
                stmt_hop(
                    site.stmt,
                    "return",
                    Some(format!("returned from {}", func.name)),
                )
            }
        };
        out.push(hop);
    }
    let h = &f.hits[hit as usize];
    let note = match &h.kind {
        HitKind::Sink {
            sink, heuristic, ..
        } => {
            let d = &cat.sinks[*sink as usize].def;
            let proc = d
                .processor
                .as_deref()
                .map(|x| format!(", processor {x}"))
                .unwrap_or_default();
            let heur = if *heuristic {
                ", matched by receiver name (heuristic)"
            } else {
                ""
            };
            format!("{} sink {}{proc}{heur}", d.class.as_str(), d.id)
        }
        HitKind::Gap { .. } => String::new(),
    };
    out.push(stmt_hop(h.stmt, "sink", Some(note)));
    // Collapse consecutive hops at the same place with the same kind.
    out.dedup_by(|b, a| {
        a.path == b.path
            && a.line == b.line
            && a.column == b.column
            && a.kind == b.kind
            && b.note.is_none()
    });
    out
}

fn sort_facts(doc: &mut Document) {
    doc.sources
        .sort_by(|a, b| a.location.cmp(&b.location).then(a.id.cmp(&b.id)));
    doc.sinks
        .sort_by(|a, b| a.location.cmp(&b.location).then(a.id.cmp(&b.id)));
    // A source or sink in a base method copied into subclasses is one, under one ID.
    doc.sources.dedup_by(|a, b| a.id == b.id);
    doc.sinks.dedup_by(|a, b| a.id == b.id);
    let src_loc: BTreeMap<&str, &Loc> = doc
        .sources
        .iter()
        .map(|s| (s.id.as_str(), &s.location))
        .collect();
    let snk_loc: BTreeMap<&str, &Loc> = doc
        .sinks
        .iter()
        .map(|s| (s.id.as_str(), &s.location))
        .collect();
    let mut keyed: Vec<((Loc, Loc, String, String), FlowRec)> = doc
        .flows
        .drain(..)
        .map(|fl| {
            let k = (
                snk_loc
                    .get(fl.sink.as_str())
                    .map(|l| (*l).clone())
                    .unwrap_or(Loc {
                        path: String::new(),
                        line: 0,
                        column: 0,
                    }),
                src_loc
                    .get(fl.source.as_str())
                    .map(|l| (*l).clone())
                    .unwrap_or(Loc {
                        path: String::new(),
                        line: 0,
                        column: 0,
                    }),
                fl.category.clone(),
                fl.id.clone(),
            );
            (k, fl)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    doc.flows = keyed.into_iter().map(|(_, f)| f).collect();
}

fn processor_declared<'a>(procs: &'a [ProcessorDecl], sink: &SinkRec) -> Option<&'a ProcessorDecl> {
    procs.iter().find(|d| {
        if let Some(p) = &sink.processor
            && d.name.eq_ignore_ascii_case(p)
        {
            return true;
        }
        if d.sinks
            .iter()
            .any(|g| Glob::api(g).is_ok_and(|g| g.is_match(&sink.catalogue_id)))
        {
            return true;
        }
        if let Some(HostRec {
            kind,
            value: Some(h),
        }) = &sink.host
            && kind == "literal"
            && d.hosts
                .iter()
                .any(|x| h == x || h.ends_with(&format!(".{x}")))
        {
            return true;
        }
        false
    })
}

/// The processor a flow to `sink` reaches, if it reaches one: the catalogue's vendor, or the
/// literal host of an outbound HTTP call.
fn processor_of(sink: &SinkRec) -> Option<String> {
    if let Some(p) = &sink.processor {
        return Some(p.clone());
    }
    match &sink.host {
        Some(HostRec {
            kind,
            value: Some(h),
        }) if kind == "literal" => Some(h.clone()),
        _ => None,
    }
}

fn is_first_party(sink: &SinkRec) -> bool {
    sink.class == SinkClass::Http && sink.host.as_ref().is_some_and(|h| h.kind == "relative")
}

type EgressKey = (Option<String>, SinkClass, Option<HostRec>);
/// (categories, flow ids, declaration)
type EgressAcc<'a> = (
    BTreeSet<String>,
    BTreeSet<String>,
    Option<&'a ProcessorDecl>,
);

/// Recompute findings, egress and summary from the facts. Dispositions already on findings are
/// kept by finding ID.
pub fn derive(doc: &mut Document, classifier: &Classifier) -> Result<()> {
    let previous: BTreeMap<String, Option<Disposition>> = doc
        .findings
        .iter()
        .map(|f| (f.id.clone(), f.disposition.clone()))
        .collect();
    let sources: BTreeMap<&str, &SourceRec> =
        doc.sources.iter().map(|s| (s.id.as_str(), s)).collect();
    let sinks: BTreeMap<&str, &SinkRec> = doc.sinks.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut findings = Vec::new();
    let setting = |rule_id: &str| doc.policy.rules.get(rule_id).copied();
    for fl in &doc.flows {
        let (Some(src), Some(snk)) = (sources.get(fl.source.as_str()), sinks.get(fl.sink.as_str()))
        else {
            bail!(
                "flow {} references a source or sink that is not in the document",
                fl.id
            );
        };
        let cat = fl.category.as_str();
        let credential = Classifier::is_credential(cat);
        let special = classifier.is_special(cat);
        let external = snk.class.is_external() && !is_first_party(snk);
        let processor = processor_of(snk);
        let declared = processor_declared(&doc.processors, snk);
        let mut fire = |rule_id: &str, message: String, proc: Option<String>| {
            let Some(s) = setting(rule_id) else { return };
            if !s.enabled {
                return;
            }
            let severity = if src.needs_review {
                s.severity.min(Severity::Info)
            } else {
                s.severity
            };
            let id = format!("pf-{}", short_hash(&serde_json::json!([rule_id, fl.id])));
            findings.push(FindingRec {
                disposition: previous.get(&id).cloned().flatten(),
                id,
                rule_id: rule_id.to_string(),
                severity,
                message,
                flow: Some(fl.id.clone()),
                gap: None,
                categories: vec![fl.category.clone()],
                processor: proc,
                needs_review: src.needs_review || snk.heuristic || cat == UNKNOWN,
                heuristic: snk.heuristic,
                location: Some(snk.location.clone()),
            });
        };
        let what = format!(
            "{cat} from `{}` ({}:{})",
            src.text, src.location.path, src.location.line
        );
        let at = format!("{}:{}", snk.location.path, snk.location.line);
        if snk.class == SinkClass::Log && !credential {
            fire(
                "PF001",
                format!("{what} reaches a log sink ({}) at {at}", snk.catalogue_id),
                None,
            );
        }
        if external
            && declared.is_none()
            && let Some(p) = &processor
        {
            fire(
                "PF002",
                format!(
                    "{what} reaches {p}, a processor not declared in .privacy-flow.yml, at {at}"
                ),
                Some(p.clone()),
            );
        }
        if snk.class == SinkClass::Llm {
            let p = processor
                .clone()
                .unwrap_or_else(|| "an LLM provider chosen at run time".into());
            fire(
                "PF003",
                format!("{what} reaches {p} through a model call at {at}"),
                processor.clone(),
            );
        }
        if special && external {
            fire(
                "PF004",
                format!(
                    "special-category data: {what} reaches an external {} sink at {at}",
                    snk.class.as_str()
                ),
                processor.clone(),
            );
        }
        // A credential written into request headers authenticates to that host; that is how
        // credentials are meant to travel over HTTP, not a leak of them.
        let as_auth_header = snk.class == SinkClass::Http
            && fl.path.iter().any(|h| {
                h.kind == "write"
                    && h.field.as_deref().is_some_and(|f| {
                        matches!(
                            f.to_ascii_lowercase().as_str(),
                            "headers"
                                | "authorization"
                                | "auth"
                                | "x-api-key"
                                | "api-key"
                                | "proxy-authorization"
                        )
                    })
            });
        if credential && (snk.class == SinkClass::Log || external) && !as_auth_header {
            fire(
                "PF005",
                format!(
                    "credentials: {what} reach a {} sink at {at}",
                    snk.class.as_str()
                ),
                processor.clone(),
            );
        }
        if snk.class == SinkClass::Http
            && snk.host.as_ref().is_some_and(|h| h.kind == "dynamic")
            && !(credential && as_auth_header)
        {
            fire(
                "PF006",
                format!("{what} is sent over HTTP to a host computed at run time, at {at}"),
                None,
            );
        }
    }
    if let Some(s) = setting("PFC01") {
        for g in &doc.coverage.gaps {
            let id = format!("pf-{}", short_hash(&serde_json::json!(["PFC01", g.id])));
            let location = g.locations.first().and_then(|l| {
                l.line.map(|line| Loc {
                    path: l.path.clone(),
                    line,
                    column: l.column.unwrap_or(1),
                })
            });
            findings.push(FindingRec {
                disposition: previous.get(&id).cloned().flatten(),
                id,
                rule_id: "PFC01".into(),
                severity: s.severity,
                message: format!("coverage gap ({}): {}", g.kind, g.detail),
                flow: None,
                gap: Some(g.id.clone()),
                categories: Vec::new(),
                processor: None,
                needs_review: false,
                heuristic: false,
                location,
            });
        }
    }
    findings.sort_by(|a, b| {
        a.rule_id
            .cmp(&b.rule_id)
            .then(a.location.cmp(&b.location))
            .then(a.id.cmp(&b.id))
    });
    findings.dedup_by(|a, b| a.id == b.id);

    // Egress: one record per (processor or host, class) for flows that leave the system.
    let mut egress: BTreeMap<EgressKey, EgressAcc> = BTreeMap::new();
    for fl in &doc.flows {
        let snk = sinks[fl.sink.as_str()];
        if !snk.class.is_external() || is_first_party(snk) {
            continue;
        }
        let processor = processor_of(snk);
        let host = if processor.is_none() {
            snk.host.clone()
        } else {
            None
        };
        let e = egress.entry((processor, snk.class, host)).or_default();
        e.0.insert(fl.category.clone());
        e.1.insert(fl.id.clone());
        if e.2.is_none() {
            e.2 = processor_declared(&doc.processors, snk);
        }
    }
    doc.egress = egress
        .into_iter()
        .map(
            |((processor, class, host), (cats, flows, decl))| EgressRec {
                fides_key: decl
                    .and_then(|d| d.fides_key.clone())
                    .or_else(|| processor.as_deref().map(fides_slug)),
                processor,
                class,
                host,
                categories: cats.into_iter().collect(),
                declared: decl.is_some(),
                citation: decl.map(|d| d.citation.clone()),
                flows: flows.into_iter().collect(),
            },
        )
        .collect();

    let mut by_rule = BTreeMap::new();
    let mut by_severity = BTreeMap::new();
    for f in &findings {
        *by_rule.entry(f.rule_id.clone()).or_insert(0) += 1;
        *by_severity
            .entry(f.severity.as_str().to_string())
            .or_insert(0) += 1;
    }
    doc.summary.flows = doc.flows.len() as u32;
    doc.summary.gaps = doc.coverage.gaps.len() as u32;
    doc.summary.findings = by_rule;
    doc.summary.by_severity = by_severity;
    doc.summary.processors = doc
        .egress
        .iter()
        .filter_map(|e| e.processor.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    doc.coverage.complete = doc.coverage.gaps.is_empty();
    doc.findings = findings;
    Ok(())
}

/// Recount the summary and keep only the egress of remaining flows (after `diff` filtering).
pub fn derive_summary_only(doc: &mut Document) {
    let flow_ids: BTreeSet<&str> = doc.flows.iter().map(|f| f.id.as_str()).collect();
    for e in &mut doc.egress {
        e.flows.retain(|f| flow_ids.contains(f.as_str()));
    }
    doc.egress.retain(|e| !e.flows.is_empty());
    let mut by_rule = BTreeMap::new();
    let mut by_severity = BTreeMap::new();
    for f in &doc.findings {
        *by_rule.entry(f.rule_id.clone()).or_insert(0) += 1;
        *by_severity
            .entry(f.severity.as_str().to_string())
            .or_insert(0) += 1;
    }
    doc.summary.flows = doc.flows.len() as u32;
    doc.summary.gaps = doc.coverage.gaps.len() as u32;
    doc.summary.findings = by_rule;
    doc.summary.by_severity = by_severity;
    doc.summary.processors = doc
        .egress
        .iter()
        .filter_map(|e| e.processor.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    doc.coverage.complete = doc.coverage.gaps.is_empty();
}

/// A Fides key for a processor name: `PostHog` → `posthog`, `api.example.com` → `api.example.com`.
pub fn fides_slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            if dash && !out.is_empty() {
                out.push('_');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    if out.is_empty() {
        "unknown_processor".into()
    } else {
        out
    }
}

/// The digest preimage: the document without its digest and without dispositions.
pub fn digest_of(doc: &Document) -> Result<String> {
    let mut d = doc.clone();
    d.digest = String::new();
    for f in &mut d.findings {
        f.disposition = None;
    }
    canonical::digest(&d)
}

pub fn seal(doc: &mut Document) -> Result<()> {
    // As digest_of, without cloning the document: dispositions are set aside and put back.
    let saved: Vec<Option<Disposition>> = doc
        .findings
        .iter_mut()
        .map(|f| f.disposition.take())
        .collect();
    doc.digest = String::new();
    let digest = canonical::digest(&*doc);
    for (f, d) in doc.findings.iter_mut().zip(saved) {
        f.disposition = d;
    }
    doc.digest = digest?;
    Ok(())
}

/// Carry dispositions over from a previous document by finding ID. Returns the IDs of
/// dispositions that no longer have a finding.
pub fn carry_dispositions(doc: &mut Document, previous: &Document) -> Vec<String> {
    let mut prev: BTreeMap<&str, &Disposition> = previous
        .findings
        .iter()
        .filter_map(|f| f.disposition.as_ref().map(|d| (f.id.as_str(), d)))
        .collect();
    for f in &mut doc.findings {
        if let Some(d) = prev.remove(f.id.as_str()) {
            f.disposition = Some(d.clone());
        }
    }
    prev.keys().map(|k| k.to_string()).collect()
}

/// Validate a disposition's shape (acc's rules). Returns a message per problem.
pub fn check_disposition(d: &Disposition) -> Vec<String> {
    let mut errs = Vec::new();
    let date = |s: &Option<String>| s.as_deref().map(is_date);
    if d.status == DispositionStatus::Open {
        return errs;
    }
    if d.owner.as_deref().is_none_or(str::is_empty) {
        errs.push("owner is required".into());
    }
    if d.rationale.as_deref().is_none_or(str::is_empty) {
        errs.push("rationale is required".into());
    }
    match date(&d.decided_at) {
        Some(true) => {}
        _ => errs.push("decided_at must be a calendar date (YYYY-MM-DD)".into()),
    }
    if date(&d.expires_at) == Some(false) {
        errs.push("expires_at must be a calendar date".into());
    }
    if date(&d.remediated_at) == Some(false) {
        errs.push("remediated_at must be a calendar date".into());
    }
    if let (Some(a), Some(e)) = (&d.decided_at, &d.expires_at)
        && e < a
    {
        errs.push("expires_at precedes decided_at".into());
    }
    if d.status == DispositionStatus::Remediated {
        match (&d.decided_at, &d.remediated_at) {
            (Some(a), Some(r)) if r >= a => {}
            (_, None) => errs.push("remediated requires remediated_at".into()),
            _ => errs.push("remediated_at precedes decided_at".into()),
        }
    }
    errs
}

pub fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().ok();
    let (Some(y), Some(m), Some(d)) = (num(0..4), num(5..7), num(8..10)) else {
        return false;
    };
    if !(1..=12).contains(&m) || d == 0 {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    d <= days[(m - 1) as usize]
}

/// Whether a disposition suppresses its finding on `as_of` (ISO dates compare as strings).
pub fn suppressed(d: &Disposition, as_of: &str) -> bool {
    match d.status {
        DispositionStatus::Open => false,
        DispositionStatus::Accepted | DispositionStatus::FalsePositive => {
            let after = d.decided_at.as_deref().is_some_and(|a| a <= as_of);
            let before = d.expires_at.as_deref().is_none_or(|e| as_of <= e);
            after && before
        }
        DispositionStatus::Remediated => d.remediated_at.as_deref().is_some_and(|r| r <= as_of),
    }
}

/// Rule metadata check used by `validate`: every finding names a known rule.
pub fn known_rule(id: &str) -> bool {
    rule(id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert!(is_date("2026-02-28"));
        assert!(!is_date("2026-02-30"));
        assert!(is_date("2024-02-29"));
        assert!(!is_date("2026-13-01"));
        assert!(!is_date("26-01-01"));
    }

    #[test]
    fn suppression_window() {
        let d = Disposition {
            status: DispositionStatus::Accepted,
            owner: Some("a".into()),
            decided_at: Some("2026-09-01".into()),
            rationale: Some("r".into()),
            expires_at: Some("2026-09-30".into()),
            remediated_at: None,
        };
        assert!(check_disposition(&d).is_empty());
        assert!(!suppressed(&d, "2026-08-31"));
        assert!(suppressed(&d, "2026-09-01"));
        assert!(suppressed(&d, "2026-09-30"));
        assert!(!suppressed(&d, "2026-10-01"));
    }

    #[test]
    fn gap_details_collapse_chains() {
        assert_eq!(
            normalize_gap_detail("db.select().where().where().where.<cb0>.and"),
            "db.select().where().where.<cb0>.and"
        );
        assert_eq!(normalize_gap_detail("m:a.b.b.b.c"), "m:a.b.c");
        assert_eq!(normalize_gap_detail("a.b.c.d.e.f.g.h.i.j"), "a.b.c.….h.i.j");
    }

    #[test]
    fn slugs() {
        assert_eq!(fides_slug("PostHog"), "posthog");
        assert_eq!(fides_slug("api.example.com"), "api.example.com");
        assert_eq!(fides_slug("Google Gemini"), "google_gemini");
    }
}
