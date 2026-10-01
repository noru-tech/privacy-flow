//! The rules: stable identifiers, default severities and the controls they map to.
//!
//! Control mappings carry identifiers and Noru's own short gloss only; no normative text from
//! any standard is quoted. They are a guide for control owners, not a compliance claim, and
//! must be checked by a privacy lawyer or Noru's compliance lead before a release.

use crate::config::Severity;

pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub severity: Severity,
    pub maps_to: &'static [&'static str],
}

pub const DOCS_BASE: &str = "https://github.com/noru-tech/privacy-flow/blob/main/docs/rules";

pub const RULES: &[Rule] = &[
    Rule {
        id: "PF001",
        name: "personal-data-to-log",
        title: "Personal data reaches a log sink",
        severity: Severity::Medium,
        maps_to: &[
            "GDPR Art. 5(1)(c)",
            "GDPR Art. 32",
            "ISO/IEC 27001:2022 A.8.15",
        ],
    },
    Rule {
        id: "PF002",
        name: "undeclared-processor",
        title: "Personal data reaches a third-party processor not declared in config",
        severity: Severity::High,
        maps_to: &["GDPR Art. 28", "GDPR Art. 30"],
    },
    Rule {
        id: "PF003",
        name: "personal-data-to-llm",
        title: "Personal data reaches an LLM provider",
        severity: Severity::Medium,
        maps_to: &[
            "GDPR Art. 28",
            "GDPR Art. 30",
            "ISO/IEC 42001:2023",
            "EU AI Act Art. 10 (data governance)",
        ],
    },
    Rule {
        id: "PF004",
        name: "special-category-to-external",
        title: "Special-category data (Art. 9 or 10) reaches an external sink",
        severity: Severity::High,
        maps_to: &["GDPR Art. 9", "GDPR Art. 10"],
    },
    Rule {
        id: "PF005",
        name: "credentials-to-log-or-third-party",
        title: "Credentials or authentication data reach a log or third party",
        severity: Severity::High,
        maps_to: &[
            "ISO/IEC 27001:2022 A.5.17",
            "ISO/IEC 27001:2022 A.8.15",
            "SOC 2 CC6.1",
        ],
    },
    Rule {
        id: "PF006",
        name: "dynamic-http-host",
        title: "Personal data reaches outbound HTTP with a non-literal host",
        severity: Severity::Warning,
        maps_to: &["GDPR Art. 30", "GDPR Art. 44"],
    },
    Rule {
        id: "PFC01",
        name: "coverage-gap",
        title: "Coverage gap: unsupported language or framework, unresolved call, or depth bound reached",
        severity: Severity::Warning,
        maps_to: &[],
    },
];

pub fn rule(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

pub fn help_uri(id: &str) -> String {
    format!("{DOCS_BASE}/{id}.md")
}
