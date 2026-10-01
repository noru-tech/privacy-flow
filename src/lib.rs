//! `privacy-flow`: a deterministic, offline static analyser that finds where personal data goes
//! in a codebase — logs, third-party SDKs, LLM providers and outbound HTTP — and reports each
//! path as a cited chain of `file:line:column` hops.
//!
//! The pipeline is: [`files`] enumerates tracked files, [`lower`] turns each into the shared
//! [`ir`], [`program`] assembles and resolves them, [`facts`] turns statements and the
//! [`catalogue`] into a flow graph, an [`engine`] finds which sources reach which sinks, and
//! [`report`] turns that into findings, flows, coverage and egress, which [`output`] renders.

pub mod analyze;
pub mod canonical;
pub mod catalogue;
pub mod classify;
pub mod cli;
pub mod config;
pub mod datamap;
pub mod engine;
pub mod facts;
pub mod files;
pub mod glob;
pub mod ir;
pub mod lower;
pub mod output;
pub mod program;
pub mod report;
pub mod resolve;
pub mod rules;

/// Process exit codes. They are part of the public interface: a code's meaning never changes,
/// and a new condition gets a new code. See `docs/exit-codes.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// Success, or the policy threshold passed.
    Ok = 0,
    /// Policy threshold exceeded (`check`).
    PolicyFailed = 1,
    /// Invalid command-line arguments.
    Usage = 2,
    /// Invalid input: configuration, document, or a file that cannot be read.
    InvalidInput = 3,
    /// Coverage incomplete: a clean result cannot be claimed (takes precedence over 1).
    CoverageIncomplete = 4,
}

#[cfg(test)]
mod tests {
    use super::Exit;

    #[test]
    fn exit_codes_are_stable() {
        assert_eq!(Exit::Ok as u8, 0);
        assert_eq!(Exit::PolicyFailed as u8, 1);
        assert_eq!(Exit::Usage as u8, 2);
        assert_eq!(Exit::InvalidInput as u8, 3);
        assert_eq!(Exit::CoverageIncomplete as u8, 4);
    }
}
