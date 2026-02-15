//! Thinking modes for configurable reasoning depth.
//!
//! Provides three levels of reasoning:
//! - Low: Fast, direct responses with minimal reasoning
//! - Medium: Balanced reasoning with tool use
//! - High: Deep reasoning with planning and extended thinking

use serde::{Deserialize, Serialize};

/// Thinking/reasoning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ThinkingMode {
    /// Fast responses, minimal reasoning. Best for simple queries.
    Low,
    /// Balanced reasoning with tool use. Default mode.
    #[default]
    Medium,
    /// Deep reasoning with planning phase. Best for complex tasks.
    High,
}

impl ThinkingMode {
    /// Get the temperature for this thinking mode.
    pub fn temperature(&self) -> f32 {
        match self {
            Self::Low => 0.3,
            Self::Medium => 0.7,
            Self::High => 0.8,
        }
    }

    /// Get the max tokens for this thinking mode.
    pub fn max_tokens(&self) -> u32 {
        match self {
            Self::Low => 1024,
            Self::Medium => 4096,
            Self::High => 8192,
        }
    }

    /// Whether planning phase should be used.
    pub fn use_planning(&self) -> bool {
        match self {
            Self::Low => false,
            Self::Medium => false,
            Self::High => true,
        }
    }

    /// Whether extended thinking/chain-of-thought is enabled.
    pub fn use_extended_thinking(&self) -> bool {
        match self {
            Self::Low => false,
            Self::Medium => false,
            Self::High => true,
        }
    }

    /// Maximum tool iterations for this mode.
    pub fn max_tool_iterations(&self) -> u32 {
        match self {
            Self::Low => 3,
            Self::Medium => 10,
            Self::High => 25,
        }
    }

    /// System prompt additions for this mode.
    pub fn system_prompt_addition(&self) -> &'static str {
        match self {
            Self::Low => "Be concise and direct. Give brief answers without extensive reasoning.",
            Self::Medium => "",
            Self::High => {
                "Think carefully and thoroughly about this task. Break down complex problems step by step. Consider multiple approaches before choosing the best one."
            }
        }
    }
}

impl std::fmt::Display for ThinkingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

impl std::str::FromStr for ThinkingMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" | "fast" | "quick" | "0" => Ok(Self::Low),
            "medium" | "balanced" | "default" | "1" | "med" => Ok(Self::Medium),
            "high" | "deep" | "thorough" | "2" => Ok(Self::High),
            _ => Err(format!(
                "Unknown thinking mode: '{}'. Use: low, medium, or high",
                s
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_medium() {
        assert_eq!(ThinkingMode::default(), ThinkingMode::Medium);
    }

    #[test]
    fn test_parse() {
        assert_eq!("low".parse::<ThinkingMode>().unwrap(), ThinkingMode::Low);
        assert_eq!("fast".parse::<ThinkingMode>().unwrap(), ThinkingMode::Low);
        assert_eq!(
            "medium".parse::<ThinkingMode>().unwrap(),
            ThinkingMode::Medium
        );
        assert_eq!("high".parse::<ThinkingMode>().unwrap(), ThinkingMode::High);
        assert_eq!("deep".parse::<ThinkingMode>().unwrap(), ThinkingMode::High);
    }

    #[test]
    fn test_properties() {
        assert!(ThinkingMode::Low.temperature() < ThinkingMode::High.temperature());
        assert!(ThinkingMode::Low.max_tokens() < ThinkingMode::High.max_tokens());
        assert!(!ThinkingMode::Low.use_planning());
        assert!(ThinkingMode::High.use_planning());
    }
}
