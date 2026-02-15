//! Skills system — modular capability bundles.
//!
//! Skills are composable packages of tools, prompts, and configuration
//! that can be assigned to agent sessions. They enable:
//! - Bundling related tools together
//! - Providing specialized system prompts
//! - Setting tool policies per-skill
//! - Sharing reusable capability sets

mod registry;
pub mod vulnerability_scanner;

pub use registry::{Skill, SkillConfig, SkillRegistry, SkillStatus, SkillTool};
pub use vulnerability_scanner::{Finding, ScanResult, ScanRule, Severity, VulnerabilityScanner};
