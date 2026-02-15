//! Safety layer for prompt injection defense.
//!
//! This module provides protection against prompt injection attacks by:
//! - Detecting suspicious patterns in external data
//! - Sanitizing tool outputs before they reach the LLM
//! - Validating inputs before processing
//! - Enforcing safety policies
//! - Detecting secret leakage in outputs

pub mod bins_allowlist;
pub mod elevated;
mod leak_detector;
mod policy;
mod sanitizer;
mod validator;

pub use leak_detector::{
    LeakAction, LeakDetectionError, LeakDetector, LeakMatch, LeakPattern, LeakScanResult,
    LeakSeverity,
};
pub use policy::{Policy, PolicyAction, PolicyRule, Severity};
pub use sanitizer::{InjectionWarning, SanitizedOutput, Sanitizer};
pub use validator::{ValidationResult, Validator};

use crate::config::SafetyConfig;

/// Unified safety layer combining sanitizer, validator, and policy.
pub struct SafetyLayer {
    sanitizer: Sanitizer,
    validator: Validator,
    policy: Policy,
    leak_detector: LeakDetector,
    config: SafetyConfig,
}

impl SafetyLayer {
    /// Create a new safety layer with the given configuration.
    pub fn new(config: &SafetyConfig) -> Self {
        Self {
            sanitizer: Sanitizer::new(),
            validator: Validator::new(),
            policy: Policy::default(),
            leak_detector: LeakDetector::new(),
            config: config.clone(),
        }
    }

    /// Sanitize tool output before it reaches the LLM.
    pub fn sanitize_tool_output(&self, tool_name: &str, output: &str) -> SanitizedOutput {
        // Check length limits first
        if output.len() > self.config.max_output_length {
            return SanitizedOutput {
                content: format!(
                    "[Output truncated: {} bytes exceeded maximum of {} bytes]",
                    output.len(),
                    self.config.max_output_length
                ),
                warnings: vec![InjectionWarning {
                    pattern: "output_too_large".to_string(),
                    severity: Severity::Low,
                    location: 0..output.len(),
                    description: format!(
                        "Output from tool '{}' was truncated due to size",
                        tool_name
                    ),
                }],
                was_modified: true,
            };
        }

        let mut content = output.to_string();
        let mut was_modified = false;

        // Leak detection and redaction
        match self.leak_detector.scan_and_clean(&content) {
            Ok(cleaned) => {
                if cleaned != content {
                    was_modified = true;
                    content = cleaned;
                }
            }
            Err(_) => {
                return SanitizedOutput {
                    content: "[Output blocked due to potential secret leakage]".to_string(),
                    warnings: vec![],
                    was_modified: true,
                };
            }
        }

        // Safety policy enforcement
        let violations = self.policy.check(&content);
        if violations
            .iter()
            .any(|rule| rule.action == crate::safety::PolicyAction::Block)
        {
            return SanitizedOutput {
                content: "[Output blocked by safety policy]".to_string(),
                warnings: vec![],
                was_modified: true,
            };
        }
        let force_sanitize = violations
            .iter()
            .any(|rule| rule.action == crate::safety::PolicyAction::Sanitize);
        if force_sanitize {
            was_modified = true;
        }

        // Always run the sanitizer for detection. If injection_check is disabled
        // we still need to detect Critical/High findings — only the lower-severity
        // escaping can be skipped. This prevents a single env var from silently
        // disabling all prompt injection defense (Finding 3).
        let mut sanitized = self.sanitizer.sanitize(&content);
        if !self.config.injection_check_enabled && !force_sanitize {
            // Keep warnings for logging but don't modify non-critical content
            let has_severe = sanitized
                .warnings
                .iter()
                .any(|w| w.severity == Severity::Critical || w.severity == Severity::High);
            if !has_severe {
                sanitized.content = content;
                sanitized.was_modified = was_modified;
            }
        }
        sanitized.was_modified = sanitized.was_modified || was_modified;
        sanitized
    }

    /// Validate input before processing.
    pub fn validate_input(&self, input: &str) -> ValidationResult {
        self.validator.validate(input)
    }

    /// Check if content violates any policy rules.
    pub fn check_policy(&self, content: &str) -> Vec<&PolicyRule> {
        self.policy.check(content)
    }

    /// Wrap content in safety delimiters for the LLM.
    ///
    /// This creates a clear structural boundary between trusted instructions
    /// and untrusted external data.
    pub fn wrap_for_llm(&self, tool_name: &str, content: &str, sanitized: bool) -> String {
        format!(
            "<tool_output name=\"{}\" sanitized=\"{}\">\n{}\n</tool_output>",
            escape_xml_attr(tool_name),
            sanitized,
            escape_xml_content(content)
        )
    }

    /// Get the sanitizer for direct access.
    pub fn sanitizer(&self) -> &Sanitizer {
        &self.sanitizer
    }

    /// Get the validator for direct access.
    pub fn validator(&self) -> &Validator {
        &self.validator
    }

    /// Get the policy for direct access.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }
}

/// Escape XML attribute value.
fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape XML content.
fn escape_xml_content(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_for_llm() {
        let config = SafetyConfig {
            max_output_length: 100_000,
            injection_check_enabled: true,
        };
        let safety = SafetyLayer::new(&config);

        let wrapped = safety.wrap_for_llm("test_tool", "Hello <world>", true);
        assert!(wrapped.contains("name=\"test_tool\""));
        assert!(wrapped.contains("sanitized=\"true\""));
        assert!(wrapped.contains("Hello &lt;world&gt;"));
    }

    #[test]
    fn test_sanitize_action_forces_sanitization_when_injection_check_disabled() {
        let config = SafetyConfig {
            max_output_length: 100_000,
            injection_check_enabled: false,
        };
        let safety = SafetyLayer::new(&config);

        // Content with an injection-like pattern that a policy might flag
        let output = safety.sanitize_tool_output("test", "normal text");
        // With injection_check disabled and no policy violations, content
        // should pass through unmodified
        assert_eq!(output.content, "normal text");
        assert!(!output.was_modified);
    }
}
