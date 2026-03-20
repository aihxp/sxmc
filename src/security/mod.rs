//! Security scanning primitives for skills and MCP servers.
//!
//! The scanner is intentionally lightweight and heuristic-based. It is useful
//! for catching obviously risky content early, not for proving something is
//! completely safe.

pub mod mcp_scanner;
pub mod patterns;
pub mod skill_scanner;

use std::fmt;

/// Severity level for security findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Warning => write!(f, "WARN"),
            Severity::Error => write!(f, "ERROR"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A single security finding.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Stable rule identifier such as `SL-INJ-001`.
    pub code: String,
    /// Severity assigned by the scanner.
    pub severity: Severity,
    /// Short human-readable label.
    pub title: String,
    /// Explanation of what was detected.
    pub description: String,
    /// Optional logical location such as a file, tool name, or MCP path.
    pub location: Option<String>,
    /// Optional 1-based line number when available.
    pub line: Option<usize>,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ({}): {}",
            self.severity, self.code, self.title, self.description
        )?;
        if let Some(ref loc) = self.location {
            write!(f, " at {}", loc)?;
        }
        if let Some(line) = self.line {
            write!(f, ":{}", line)?;
        }
        Ok(())
    }
}

/// A security scan report.
#[derive(Debug, Clone)]
pub struct ScanReport {
    /// The scanned target, such as a skill name or MCP server identifier.
    pub target: String,
    /// All findings produced for that target.
    pub findings: Vec<Finding>,
}

impl ScanReport {
    /// Create an empty report for a named target.
    pub fn new(target: &str) -> Self {
        Self {
            target: target.to_string(),
            findings: Vec::new(),
        }
    }

    /// Add a single finding to the report.
    pub fn add(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// Return a copy of the report containing only findings at or above `min`.
    pub fn filtered(&self, min: Severity) -> Self {
        Self {
            target: self.target.clone(),
            findings: self
                .findings
                .iter()
                .filter(|f| f.severity >= min)
                .cloned()
                .collect(),
        }
    }

    /// Get findings filtered by minimum severity.
    pub fn findings_at_severity(&self, min: Severity) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.severity >= min).collect()
    }

    pub fn has_critical(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == Severity::Critical)
    }

    /// Whether the report contains any `Error` or `Critical` findings.
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity >= Severity::Error)
    }

    /// Whether the scanner produced zero findings.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// Format report as human-readable text.
    pub fn format_text(&self) -> String {
        if self.findings.is_empty() {
            return format!("[PASS] {} — no security issues found", self.target);
        }

        let mut lines = vec![format!(
            "[SCAN] {} — {} issue(s) found",
            self.target,
            self.findings.len()
        )];

        let mut sorted = self.findings.clone();
        sorted.sort_by(|a, b| b.severity.cmp(&a.severity));

        for f in &sorted {
            lines.push(format!("  {}", f));
        }

        lines.join("\n")
    }

    /// Format report as JSON.
    pub fn format_json(&self) -> serde_json::Value {
        serde_json::json!({
            "target": self.target,
            "findings": self.findings.iter().map(|f| {
                let mut obj = serde_json::json!({
                    "code": f.code,
                    "severity": format!("{}", f.severity),
                    "title": f.title,
                    "description": f.description,
                });
                if let Some(ref loc) = f.location {
                    obj["location"] = serde_json::json!(loc);
                }
                if let Some(line) = f.line {
                    obj["line"] = serde_json::json!(line);
                }
                obj
            }).collect::<Vec<_>>(),
            "summary": {
                "total": self.findings.len(),
                "critical": self.findings.iter().filter(|f| f.severity == Severity::Critical).count(),
                "error": self.findings.iter().filter(|f| f.severity == Severity::Error).count(),
                "warning": self.findings.iter().filter(|f| f.severity == Severity::Warning).count(),
                "info": self.findings.iter().filter(|f| f.severity == Severity::Info).count(),
            }
        })
    }
}
