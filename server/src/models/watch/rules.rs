//! Account-owned observation rules. All comparisons use the same deterministic evaluator.
use super::{Band, BandReference};
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandMode {
    Near,
    AtOrBelow,
    AtOrAbove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BandRule {
    pub reference: BandReference,
    pub mode: BandMode,
    pub offset_bps: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WatchRules {
    pub min_yield_bps: u32,
    pub daily: BandRule,
    pub weekly: BandRule,
}

impl Default for WatchRules {
    fn default() -> Self {
        Self {
            min_yield_bps: (super::MIN_YIELD_PCT * 100.0) as u32,
            daily: BandRule {
                reference: BandReference::Lower,
                mode: BandMode::Near,
                offset_bps: (super::BAND_DISTANCE_PCT * 100.0) as i32,
            },
            weekly: BandRule {
                reference: BandReference::Middle,
                mode: BandMode::AtOrBelow,
                offset_bps: 0,
            },
        }
    }
}

impl WatchRules {
    pub fn validate(&self) -> AppResult<()> {
        if !(1..=10000).contains(&self.min_yield_bps)
            || [self.daily, self.weekly].iter().any(|rule| {
                !(-9900..=10000).contains(&rule.offset_bps)
                    || (rule.mode == BandMode::Near && !(0..=9900).contains(&rule.offset_bps))
            })
        {
            return Err(AppError::Validation("股息率须为 0.01% 至 100%；附近偏离须为 0% 至 99%，单向偏移须为 -99% 至 100%，最多两位小数".into()));
        }
        Ok(())
    }

    pub fn description(&self) -> String {
        format!(
            "已实施税前现金分红TTM股息率至少{}%；日线{}；周线{}",
            self.min_yield_bps as f64 / 100.0,
            self.daily.description(),
            self.weekly.description()
        )
    }
}

impl BandRule {
    pub fn description(&self) -> String {
        let reference = match self.reference {
            BandReference::Lower => "下轨",
            BandReference::Middle => "中轨",
            BandReference::Upper => "上轨",
        };
        let pct = self.offset_bps as f64 / 100.0;
        match self.mode {
            BandMode::Near => format!("BOLL(20,2){reference} ±{pct}%"),
            BandMode::AtOrBelow => format!("收盘价≤BOLL(20,2){reference}×(1+{pct}%)"),
            BandMode::AtOrAbove => format!("收盘价≥BOLL(20,2){reference}×(1+{pct}%)"),
        }
    }
}

impl Band {
    pub(super) fn apply_rule(&mut self, price: f64, rule: &BandRule) {
        self.reference = rule.reference;
        let reference = match rule.reference {
            BandReference::Lower => self.lower,
            BandReference::Middle => self.middle,
            BandReference::Upper => self.upper,
        };
        self.distance_pct =
            (reference.is_finite() && reference > 0.0 && price.is_finite() && price > 0.0)
                .then(|| (price / reference - 1.0) * 100.0);
        let offset = rule.offset_bps as f64 / 100.0;
        self.matches_rule = self.distance_pct.is_some_and(|distance| match rule.mode {
            BandMode::Near => distance.abs() <= offset + 1e-9,
            BandMode::AtOrBelow => distance <= offset + 1e-9,
            BandMode::AtOrAbove => distance >= offset - 1e-9,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::watch::{
        dividend_prices, evaluate, tests::market_fixture, WatchIssue, WatchStatus,
    };

    #[test]
    fn references_modes_offsets_and_inclusive_boundaries_share_one_comparison() {
        for (reference, target) in [
            (BandReference::Lower, 80.0),
            (BandReference::Middle, 100.0),
            (BandReference::Upper, 120.0),
        ] {
            let mut band = Band {
                lower: 80.0,
                middle: 100.0,
                upper: 120.0,
                distance_pct: None,
                reference,
                matches_rule: false,
            };
            for (mode, offset_bps, passing, failing) in [
                (BandMode::Near, 300, target * 0.97, target * 0.9699),
                (BandMode::Near, 300, target * 1.03, target * 1.0301),
                (BandMode::AtOrBelow, -200, target * 0.98, target * 0.9801),
                (BandMode::AtOrAbove, 200, target * 1.02, target * 1.0199),
            ] {
                let rule = BandRule {
                    reference,
                    mode,
                    offset_bps,
                };
                band.apply_rule(passing, &rule);
                assert!(band.matches_rule, "{rule:?} {passing}");
                band.apply_rule(failing, &rule);
                assert!(!band.matches_rule, "{rule:?} {failing}");
            }
        }
    }

    #[test]
    fn custom_dividend_threshold_is_exact_and_reference_levels_include_it_once() {
        let (mut data, day) = market_fixture();
        data.dividends.as_mut().unwrap()[0].cash_per_ten = "27.5".into();
        let rules = WatchRules {
            min_yield_bps: 275,
            ..WatchRules::default()
        };
        let passed = evaluate(&rules, "600036.SS", &data, day, String::new());
        assert_eq!(passed.yield_matches_rule, Some(true));
        assert_eq!(passed.status, WatchStatus::Attention);
        let levels = dividend_prices(Some(&passed));
        let threshold = levels.iter().find(|p| p.yield_pct == 2.75).unwrap();
        assert_eq!(threshold.price, "100.00");
        assert_eq!(threshold.reached, Some(true));
        assert_eq!(levels.len(), 5);
        data.dividends.as_mut().unwrap()[0].cash_per_ten = "27.49999999999999".into();
        let below = evaluate(&rules, "600036.SS", &data, day, String::new());
        assert_eq!(below.yield_matches_rule, Some(false));
        assert_eq!(below.status, WatchStatus::Waiting);
        let mut default_projection = passed.clone();
        default_projection.apply_rules(&WatchRules::default());
        assert_eq!(dividend_prices(Some(&default_projection)).len(), 4);
    }

    #[test]
    fn only_the_selected_reference_can_block_a_band_and_unknown_dividends_stay_unknown() {
        let (data, day) = market_fixture();
        let mut observation = evaluate(
            &WatchRules::default(),
            "600036.SS",
            &data,
            day,
            String::new(),
        );
        observation.daily.as_mut().unwrap().lower = -1.0;
        observation.apply_rules(&WatchRules::default());
        assert!(observation.issues.contains(&WatchIssue::NonPositiveBand));
        let mut rules = WatchRules::default();
        rules.daily.reference = BandReference::Upper;
        observation.apply_rules(&rules);
        assert_eq!(observation.status, WatchStatus::Attention);
        assert!(!observation.issues.contains(&WatchIssue::NonPositiveBand));
        let mut missing = data;
        missing.dividends = None;
        let missing = evaluate(&rules, "600036.SS", &missing, day, String::new());
        assert_eq!(missing.yield_matches_rule, None);
        assert_eq!(missing.status, WatchStatus::Incomplete);
    }

    #[test]
    fn invalid_ranges_fractional_basis_points_and_unknown_modes_are_rejected() {
        for bps in [0, 10001] {
            assert!(WatchRules {
                min_yield_bps: bps,
                ..WatchRules::default()
            }
            .validate()
            .is_err());
        }
        for offset in [-1, 9901] {
            let mut rules = WatchRules::default();
            rules.daily.offset_bps = offset;
            assert!(rules.validate().is_err());
        }
        let mut value = serde_json::to_value(WatchRules::default()).unwrap();
        value["daily"]["mode"] = "unknown".into();
        assert!(serde_json::from_value::<WatchRules>(value).is_err());
        let mut value = serde_json::to_value(WatchRules::default()).unwrap();
        value["minYieldBps"] = 333.3.into();
        assert!(serde_json::from_value::<WatchRules>(value).is_err());
    }
}
