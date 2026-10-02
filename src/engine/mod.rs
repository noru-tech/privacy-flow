//! Flow engines.
//!
//! Both engines answer the same question over the same [`Facts`]: which seeds reach which
//! hits, within the call-depth bound. [`worklist`] also reconstructs one shortest witness path
//! per (seed, hit) and is the engine `scan` uses; [`datalog`] computes the same reachability
//! as a Datalog program (ascent) and is kept as an independent oracle: the test suite requires
//! the two to agree on every fixture. The comparison that decided this split is in
//! `docs/adr/0002-flow-engine.md`.
//!
//! ## Semantics (normative for both engines)
//!
//! Nodes are `(variable, token)`. Within the program's edges:
//!
//! - `Copy a→b`: `(a,t)` → `(b,t)`.
//! - `Collapse a→b`: `(a,t)` → `(b,TOP)`.
//! - `Load(k) a→b`: `(a,TOP)` and `(a,k)` → `(b,TOP)`; in a summary, `(a,PHI)` → `(b,TOP)`
//!   when the summary's owner never reads `k`.
//! - `Store(k) a→b`: `(a,t)` → `(b,k)`.
//! - `Sanitize(s) a→b`: like `Collapse`, unless `s` removes the seed's category.
//!
//! Calls to local functions are not edges. A function's **return slots** are its return value
//! and the field variables (and rests) of the allocation sites it returns (ADR 0009); each slot
//! has an **exit** at each call site, to a variable of the caller: the return value to the
//! call's result, a site's field variable to the call site's clone of it.
//!
//! A function's **summary** records, for each formal parameter and token, which return slots
//! (with their tokens), hits and shared variables (module-level bindings and class instances)
//! the formal reaches, applying callees' summaries at nested call sites, where a slot reached
//! continues at that slot's exit for that call site. A summary stops at shared variables,
//! except the owner's own return slots. From a seed, the search follows edges, applies
//! summaries at call sites, continues from shared variables, and follows each return slot to
//! its exit at every call site (a slot reached outside a summary only holds data that
//! originated inside the function or in shared state, so this is never an unrealizable path).
//!
//! **Depth:** every entry into a callee and every return out of one counts as one call
//! boundary. A witness may cross at most `max_depth` of them; where the bound stops a witness,
//! a `depth_bound` hit at that call site is reported instead, so a bound never silently
//! becomes "no flow".

pub mod datalog;
pub mod worklist;

use crate::facts::Token;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    /// An edge created by this statement.
    Edge(u32),
    /// An argument passed to formal `formal` at call site `site`.
    Enter { site: u32, formal: u32 },
    /// A value returned to call site `site`.
    Exit { site: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    Hit(u32),
    /// The depth bound stopped a witness at this call site.
    DepthBound(u32),
}

#[derive(Clone, Debug)]
pub struct Reach {
    pub seed: u32,
    pub target: Target,
    pub depth: u32,
    pub path: Vec<Step>,
}

/// The default call-depth bound. Measured on a 220k-line TypeScript monorepo: at 8 the bound
/// cut 451 witnesses, at 16 it cut 56, at 24 two, at 32 none, with no measurable change in run
/// time (see NOTES.md).
pub const DEFAULT_MAX_DEPTH: u32 = 32;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub max_depth: u32,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

/// The node a seed starts from, for engines that only need reachability.
pub fn seed_node(s: &crate::facts::Seed) -> (u32, Token) {
    (s.var, s.token)
}
