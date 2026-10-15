//! What a variable's value is known to be, from the statements that define it: the plain copy
//! it came from, the call that made it, or the literal text it starts with. Shared by the facts
//! builder (hosts, names) and the topology pass (client registrations).

use crate::program::{GArg, GKind, GPart, Program, Prov, VarId};

/// The statements that define each variable.
#[derive(Default)]
pub struct Defs(Vec<Vec<u32>>);

impl Defs {
    pub fn build(p: &Program) -> Self {
        let mut defs = vec![Vec::new(); p.vars.len()];
        for (i, s) in p.stmts.iter().enumerate() {
            let dst = match &s.kind {
                GKind::Copy { dst, .. }
                | GKind::Lit { dst, .. }
                | GKind::Load { dst, .. }
                | GKind::Concat { dst, .. }
                | GKind::Call { dst, .. }
                | GKind::ThisLoad { dst, .. }
                | GKind::FuncRef { dst, .. }
                | GKind::ClassRef { dst, .. }
                | GKind::Global { dst, .. } => *dst,
                _ => continue,
            };
            defs[dst as usize].push(i as u32);
        }
        Defs(defs)
    }

    /// The defining statements of every variable, by variable.
    pub fn all(&self) -> &[Vec<u32>] {
        &self.0
    }

    /// The statements that define `var`.
    pub fn of(&self, var: VarId) -> &[u32] {
        &self.0[var as usize]
    }

    /// What a variable was copied from, through single plain copies.
    pub fn copy_origin(&self, p: &Program, var: VarId) -> VarId {
        self.copy_origin_at(p, var, 0)
    }

    fn copy_origin_at(&self, p: &Program, var: VarId, depth: u32) -> VarId {
        match (self.of(var), depth < 8) {
            ([d], true) => match &p.stmts[*d as usize].kind {
                GKind::Copy { src, .. } => self.copy_origin_at(p, *src, depth + 1),
                _ => var,
            },
            _ => var,
        }
    }

    /// The call that defines a variable, through plain copies: (its arguments, its API paths).
    pub fn defining_call(&self, p: &Program, var: VarId) -> Option<(Vec<GArg>, Vec<String>)> {
        self.defining_call_at(p, var, 0)
    }

    fn defining_call_at(
        &self,
        p: &Program,
        var: VarId,
        depth: u32,
    ) -> Option<(Vec<GArg>, Vec<String>)> {
        if depth > 8 {
            return None;
        }
        let [stmt] = self.of(var) else {
            return None;
        };
        match &p.stmts[*stmt as usize].kind {
            GKind::Copy { src, .. } => self.defining_call_at(p, *src, depth + 1),
            GKind::Call { args, callee, .. } => {
                let mut paths: Vec<String> = Vec::new();
                if let Some(t) = p.calls.get(stmt) {
                    paths.extend(t.apis.iter().map(|s| p.syms.str(*s).to_string()));
                }
                if let crate::program::GCallee::Value(v) = callee {
                    paths.extend(api_paths(p, *v));
                }
                Some((args.clone(), paths))
            }
            _ => None,
        }
    }

    /// The literal text a variable is known to start with, and whether that is all of it.
    pub fn literal_prefix(&self, p: &Program, var: VarId) -> Option<(String, bool)> {
        self.literal_prefix_at(p, var, 0)
    }

    fn literal_prefix_at(&self, p: &Program, var: VarId, depth: u32) -> Option<(String, bool)> {
        if depth > 8 {
            return None;
        }
        let [stmt] = self.of(var) else {
            return None;
        };
        match &p.stmts[*stmt as usize].kind {
            GKind::Lit { value: Some(v), .. } => Some((v.clone(), true)),
            GKind::Copy { src, .. } => self.literal_prefix_at(p, *src, depth + 1),
            GKind::Concat { parts, .. } => {
                let mut out = String::new();
                for (i, part) in parts.iter().enumerate() {
                    match part {
                        GPart::Lit(s) => out.push_str(s),
                        GPart::Var(v) => match self.literal_prefix_at(p, *v, depth + 1) {
                            Some((s, true)) => out.push_str(&s),
                            Some((s, false)) => {
                                out.push_str(&s);
                                return Some((out, false));
                            }
                            None => {
                                return if i == 0 && out.is_empty() {
                                    None
                                } else {
                                    Some((out, false))
                                };
                            }
                        },
                    }
                }
                Some((out, true))
            }
            _ => None,
        }
    }
}

/// The API paths a variable's provenance names.
pub fn api_paths(p: &Program, var: VarId) -> impl Iterator<Item = String> + '_ {
    p.prov[var as usize].iter().filter_map(|pr| match pr {
        Prov::Api(s) => Some(p.syms.str(*s).to_string()),
        _ => None,
    })
}
