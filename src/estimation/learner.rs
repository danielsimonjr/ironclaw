//! Statistical learning for estimation improvement.

use std::collections::HashMap;
use std::time::Duration;

use rust_decimal::Decimal;

/// Learning model for estimation adjustments.
#[derive(Debug, Clone)]
pub struct LearningModel {
    /// Cost adjustment factor (multiplier).
    pub cost_factor: f64,
    /// Time adjustment factor (multiplier).
    pub time_factor: f64,
    /// Number of samples.
    pub sample_count: u64,
    /// Running error rate for cost.
    pub cost_error_rate: f64,
    /// Running error rate for time.
    pub time_error_rate: f64,
}

impl Default for LearningModel {
    fn default() -> Self {
        Self {
            cost_factor: 1.0,
            time_factor: 1.0,
            sample_count: 0,
            cost_error_rate: 0.0,
            time_error_rate: 0.0,
        }
    }
}

/// Learner that improves estimates over time.
pub struct EstimationLearner {
    /// Models per category.
    models: HashMap<String, LearningModel>,
    /// Exponential moving average alpha.
    alpha: f64,
    /// Minimum samples before adjusting.
    min_samples: u64,
}

impl EstimationLearner {
    /// Create a new estimation learner.
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            alpha: 0.1, // EMA smoothing factor
            min_samples: 5,
        }
    }

    /// Record actual results and update the model.
    pub fn record(
        &mut self,
        category: &str,
        estimated_cost: Decimal,
        actual_cost: Decimal,
        estimated_time: Duration,
        actual_time: Duration,
    ) {
        let model = self.models.entry(category.to_string()).or_default();
        model.sample_count += 1;

        // Calculate errors
        let cost_ratio = if !estimated_cost.is_zero() {
            (actual_cost / estimated_cost)
                .to_string()
                .parse::<f64>()
                .unwrap_or(1.0)
        } else {
            1.0
        };

        let time_ratio = if !estimated_time.is_zero() {
            actual_time.as_secs_f64() / estimated_time.as_secs_f64()
        } else {
            1.0
        };

        // Update factors using exponential moving average
        model.cost_factor = model.cost_factor * (1.0 - self.alpha) + cost_ratio * self.alpha;
        model.time_factor = model.time_factor * (1.0 - self.alpha) + time_ratio * self.alpha;

        // Update error rates
        let cost_error = (cost_ratio - 1.0).abs();
        let time_error = (time_ratio - 1.0).abs();

        model.cost_error_rate =
            model.cost_error_rate * (1.0 - self.alpha) + cost_error * self.alpha;
        model.time_error_rate =
            model.time_error_rate * (1.0 - self.alpha) + time_error * self.alpha;
    }

    /// Adjust estimates based on learned factors.
    pub fn adjust(&self, category: &str, cost: Decimal, time: Duration) -> (Decimal, Duration) {
        let model = self.models.get(category);

        match model {
            Some(m) if m.sample_count >= self.min_samples => {
                let adjusted_cost = cost * Decimal::try_from(m.cost_factor).unwrap_or(Decimal::ONE);
                let adjusted_time = Duration::from_secs_f64(time.as_secs_f64() * m.time_factor);
                (adjusted_cost, adjusted_time)
            }
            _ => (cost, time), // Not enough data, use original estimates
        }
    }

    /// Get confidence for a category (based on sample count and error rate).
    pub fn confidence(&self, category: &str) -> f64 {
        match self.models.get(category) {
            Some(m) if m.sample_count >= self.min_samples => {
                // Higher samples and lower error = higher confidence
                let sample_factor = (m.sample_count as f64 / 100.0).min(1.0);
                let error_factor = 1.0 - ((m.cost_error_rate + m.time_error_rate) / 2.0).min(1.0);
                0.5 + (sample_factor * 0.3) + (error_factor * 0.2)
            }
            Some(_) => 0.3, // Some data but not enough
            None => 0.2,    // No data
        }
    }

    /// Get the model for a category.
    pub fn get_model(&self, category: &str) -> Option<&LearningModel> {
        self.models.get(category)
    }

    /// Get all models.
    pub fn all_models(&self) -> &HashMap<String, LearningModel> {
        &self.models
    }

    /// Set the EMA alpha.
    pub fn set_alpha(&mut self, alpha: f64) {
        self.alpha = alpha.clamp(0.01, 0.5);
    }

    /// Set minimum samples.
    pub fn set_min_samples(&mut self, min: u64) {
        self.min_samples = min;
    }

    /// Clear all learned data.
    pub fn clear(&mut self) {
        self.models.clear();
    }
}

impl Default for EstimationLearner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_learning_model_update() {
        let mut learner = EstimationLearner::new();
        learner.set_min_samples(2);

        // Record some results where actuals are 20% higher than estimates
        for _ in 0..5 {
            learner.record(
                "test",
                dec!(100.0),
                dec!(120.0),
                Duration::from_secs(60),
                Duration::from_secs(72),
            );
        }

        let model = learner.get_model("test").unwrap();
        assert!(model.cost_factor > 1.0);
        assert!(model.time_factor > 1.0);
    }

    #[test]
    fn test_adjustment() {
        let mut learner = EstimationLearner::new();
        learner.set_min_samples(2);

        // Train with consistent 50% underestimation
        for _ in 0..10 {
            learner.record(
                "test",
                dec!(100.0),
                dec!(150.0),
                Duration::from_secs(60),
                Duration::from_secs(90),
            );
        }

        let (adjusted_cost, adjusted_time) =
            learner.adjust("test", dec!(100.0), Duration::from_secs(60));

        // Should adjust upward
        assert!(adjusted_cost > dec!(100.0));
        assert!(adjusted_time > Duration::from_secs(60));
    }

    #[test]
    fn test_confidence() {
        let mut learner = EstimationLearner::new();

        // No data = low confidence
        assert!(learner.confidence("unknown") < 0.5);

        // Add data
        for _ in 0..20 {
            learner.record(
                "known",
                dec!(100.0),
                dec!(100.0), // Perfect estimates
                Duration::from_secs(60),
                Duration::from_secs(60),
            );
        }

        // More data with good accuracy = higher confidence
        assert!(learner.confidence("known") > 0.5);
    }

    // ==================== Zero-estimate guard tests ====================

    #[test]
    fn test_zero_estimated_cost_guard() {
        let mut learner = EstimationLearner::new();

        // Zero estimated cost should not cause division by zero
        learner.record(
            "zero_cost",
            dec!(0.0),   // zero estimate
            dec!(100.0), // non-zero actual
            Duration::from_secs(60),
            Duration::from_secs(60),
        );

        let model = learner.get_model("zero_cost").unwrap();
        // cost_ratio defaults to 1.0 when estimated is zero
        // cost_factor should be 1.0 * (1 - 0.1) + 1.0 * 0.1 = 1.0
        assert!(
            (model.cost_factor - 1.0).abs() < 0.01,
            "cost_factor should remain ~1.0 with zero estimate, got {}",
            model.cost_factor
        );
    }

    #[test]
    fn test_zero_estimated_time_guard() {
        let mut learner = EstimationLearner::new();

        // Zero estimated time should not cause division by zero
        learner.record(
            "zero_time",
            dec!(100.0),
            dec!(100.0),
            Duration::ZERO,          // zero estimate
            Duration::from_secs(60), // non-zero actual
        );

        let model = learner.get_model("zero_time").unwrap();
        // time_ratio defaults to 1.0 when estimated is zero
        assert!(
            (model.time_factor - 1.0).abs() < 0.01,
            "time_factor should remain ~1.0 with zero estimate, got {}",
            model.time_factor
        );
    }

    #[test]
    fn test_both_zero_estimates() {
        let mut learner = EstimationLearner::new();

        // Both zero should not panic
        learner.record(
            "both_zero",
            dec!(0.0),
            dec!(0.0),
            Duration::ZERO,
            Duration::ZERO,
        );

        let model = learner.get_model("both_zero").unwrap();
        assert_eq!(model.sample_count, 1);
    }

    // ==================== EMA correctness tests ====================

    #[test]
    fn test_ema_single_sample() {
        let mut learner = EstimationLearner::new();
        // alpha = 0.1 by default
        // Record: estimated 100, actual 200 → cost_ratio = 2.0
        learner.record(
            "ema",
            dec!(100.0),
            dec!(200.0),
            Duration::from_secs(100),
            Duration::from_secs(200),
        );

        let model = learner.get_model("ema").unwrap();
        // cost_factor = 1.0 * (1 - 0.1) + 2.0 * 0.1 = 0.9 + 0.2 = 1.1
        assert!(
            (model.cost_factor - 1.1).abs() < 0.001,
            "EMA cost_factor should be 1.1, got {}",
            model.cost_factor
        );
        // time_factor = 1.0 * 0.9 + 2.0 * 0.1 = 1.1
        assert!(
            (model.time_factor - 1.1).abs() < 0.001,
            "EMA time_factor should be 1.1, got {}",
            model.time_factor
        );
    }

    #[test]
    fn test_ema_converges_with_consistent_data() {
        let mut learner = EstimationLearner::new();

        // Record many samples where actual is always 1.5x the estimate
        for _ in 0..100 {
            learner.record(
                "converge",
                dec!(100.0),
                dec!(150.0),
                Duration::from_secs(100),
                Duration::from_secs(150),
            );
        }

        let model = learner.get_model("converge").unwrap();
        // Should converge to 1.5
        assert!(
            (model.cost_factor - 1.5).abs() < 0.05,
            "cost_factor should converge to ~1.5, got {}",
            model.cost_factor
        );
        assert!(
            (model.time_factor - 1.5).abs() < 0.05,
            "time_factor should converge to ~1.5, got {}",
            model.time_factor
        );
    }

    // ==================== Alpha clamping tests ====================

    #[test]
    fn test_set_alpha_clamped() {
        let mut learner = EstimationLearner::new();

        learner.set_alpha(0.0); // below min 0.01
        // Record to verify it doesn't panic
        learner.record(
            "alpha_low",
            dec!(100.0),
            dec!(100.0),
            Duration::from_secs(60),
            Duration::from_secs(60),
        );
        let model = learner.get_model("alpha_low").unwrap();
        assert_eq!(model.sample_count, 1);

        learner.set_alpha(1.0); // above max 0.5
        learner.record(
            "alpha_high",
            dec!(100.0),
            dec!(200.0),
            Duration::from_secs(60),
            Duration::from_secs(120),
        );
        let model = learner.get_model("alpha_high").unwrap();
        assert_eq!(model.sample_count, 1);
    }

    // ==================== min_samples threshold tests ====================

    #[test]
    fn test_adjust_below_min_samples_returns_original() {
        let mut learner = EstimationLearner::new();
        // Default min_samples = 5

        // Only record 3 samples
        for _ in 0..3 {
            learner.record(
                "few",
                dec!(100.0),
                dec!(200.0), // 2x underestimate
                Duration::from_secs(60),
                Duration::from_secs(120),
            );
        }

        let (cost, time) = learner.adjust("few", dec!(100.0), Duration::from_secs(60));
        // Should return originals since sample_count < min_samples
        assert_eq!(cost, dec!(100.0));
        assert_eq!(time, Duration::from_secs(60));
    }

    #[test]
    fn test_adjust_above_min_samples_adjusts() {
        let mut learner = EstimationLearner::new();

        // Record 10 samples (above min_samples=5)
        for _ in 0..10 {
            learner.record(
                "enough",
                dec!(100.0),
                dec!(150.0),
                Duration::from_secs(60),
                Duration::from_secs(90),
            );
        }

        let (cost, time) = learner.adjust("enough", dec!(100.0), Duration::from_secs(60));
        // Should adjust upward
        assert!(cost > dec!(100.0));
        assert!(time > Duration::from_secs(60));
    }

    #[test]
    fn test_adjust_unknown_category_returns_original() {
        let learner = EstimationLearner::new();
        let (cost, time) = learner.adjust("nonexistent", dec!(50.0), Duration::from_secs(30));
        assert_eq!(cost, dec!(50.0));
        assert_eq!(time, Duration::from_secs(30));
    }

    // ==================== Confidence level tests ====================

    #[test]
    fn test_confidence_no_data() {
        let learner = EstimationLearner::new();
        assert!((learner.confidence("unknown") - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_confidence_insufficient_data() {
        let mut learner = EstimationLearner::new();
        // Record fewer than min_samples (5)
        for _ in 0..3 {
            learner.record(
                "few",
                dec!(100.0),
                dec!(100.0),
                Duration::from_secs(60),
                Duration::from_secs(60),
            );
        }
        assert!((learner.confidence("few") - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_confidence_increases_with_samples() {
        let mut learner = EstimationLearner::new();
        for _ in 0..50 {
            learner.record(
                "many",
                dec!(100.0),
                dec!(100.0),
                Duration::from_secs(60),
                Duration::from_secs(60),
            );
        }
        let conf = learner.confidence("many");
        assert!(
            conf > 0.5,
            "Confidence with many perfect samples should be > 0.5, got {}",
            conf
        );
    }

    // ==================== Sample counting ====================

    #[test]
    fn test_sample_count_increments() {
        let mut learner = EstimationLearner::new();
        for i in 1..=5 {
            learner.record(
                "count",
                dec!(100.0),
                dec!(100.0),
                Duration::from_secs(60),
                Duration::from_secs(60),
            );
            assert_eq!(learner.get_model("count").unwrap().sample_count, i);
        }
    }

    // ==================== Clear tests ====================

    #[test]
    fn test_clear_removes_all_models() {
        let mut learner = EstimationLearner::new();
        learner.record(
            "a",
            dec!(1.0),
            dec!(1.0),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        learner.record(
            "b",
            dec!(1.0),
            dec!(1.0),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        assert_eq!(learner.all_models().len(), 2);
        learner.clear();
        assert!(learner.all_models().is_empty());
    }

    // ==================== Error rate tracking ====================

    #[test]
    fn test_error_rate_with_perfect_estimates() {
        let mut learner = EstimationLearner::new();
        for _ in 0..10 {
            learner.record(
                "perfect",
                dec!(100.0),
                dec!(100.0),
                Duration::from_secs(60),
                Duration::from_secs(60),
            );
        }
        let model = learner.get_model("perfect").unwrap();
        assert!(
            model.cost_error_rate < 0.01,
            "cost_error_rate should be near 0 with perfect estimates, got {}",
            model.cost_error_rate
        );
        assert!(
            model.time_error_rate < 0.01,
            "time_error_rate should be near 0 with perfect estimates, got {}",
            model.time_error_rate
        );
    }

    #[test]
    fn test_error_rate_with_inaccurate_estimates() {
        let mut learner = EstimationLearner::new();
        for _ in 0..20 {
            learner.record(
                "inaccurate",
                dec!(100.0),
                dec!(300.0), // 3x underestimate
                Duration::from_secs(60),
                Duration::from_secs(180),
            );
        }
        let model = learner.get_model("inaccurate").unwrap();
        assert!(
            model.cost_error_rate > 1.0,
            "cost_error_rate should be significant with 3x underestimate, got {}",
            model.cost_error_rate
        );
    }
}
