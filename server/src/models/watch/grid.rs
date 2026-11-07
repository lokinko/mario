use super::{positive_ttm_dividend, Observation};
use crate::{AppError, AppResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridLevel {
    pub yield_bps: u16,
    pub buy_shares: u32,
    pub sell_shares: u32,
}

/// Generation settings are a draft aid; explicit levels are the saved instructions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridGenerator {
    pub start_yield_bps: u16,
    pub end_yield_bps: u16,
    pub step_bps: u16,
    pub initial_buy_shares: u32,
    pub initial_sell_shares: u32,
    pub buy_growth_bps: i32,
    pub sell_growth_bps: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridPlan {
    pub budget: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<GridGenerator>,
    pub levels: Vec<GridLevel>,
}

impl GridPlan {
    pub fn validate(&self) -> AppResult<()> {
        let invalid = |message: &str| AppError::Validation(message.into());
        if Decimal::from_str_exact(&self.budget)
            .ok()
            .is_none_or(|budget| {
                budget <= Decimal::ZERO
                    || budget > Decimal::from(1_000_000_000_000_u64)
                    || budget.scale() > 2
            })
        {
            return Err(invalid(
                "请填写大于 0、最多两位小数且不超过一万亿元的配置金额",
            ));
        }
        if self.levels.is_empty() || self.levels.len() > 100 {
            return Err(invalid("请设置 1 至 100 个网格档位"));
        }
        let mut yields = BTreeSet::new();
        for level in &self.levels {
            if !(1..=10_000).contains(&level.yield_bps) || !yields.insert(level.yield_bps) {
                return Err(invalid("股息率须为 0.01% 至 100% 内不重复的档位"));
            }
            if level.buy_shares > 1_000_000_000 || level.sell_shares > 1_000_000_000 {
                return Err(invalid("每档买卖股数须为 0 至 10 亿之间的整数"));
            }
        }
        if !self.levels.iter().any(|level| level.buy_shares > 0) {
            return Err(invalid("至少填写一档买入股数"));
        }
        if let Some(g) = &self.generator {
            if g.start_yield_bps == 0
                || g.end_yield_bps > 10_000
                || g.start_yield_bps >= g.end_yield_bps
                || g.step_bps == 0
                || (g.end_yield_bps - g.start_yield_bps) % g.step_bps != 0
                || (g.end_yield_bps - g.start_yield_bps) / g.step_bps >= 100
                || g.initial_buy_shares > 1_000_000_000
                || g.initial_sell_shares > 1_000_000_000
                || !(-10_000..=100_000).contains(&g.buy_growth_bps)
                || !(-10_000..=100_000).contains(&g.sell_growth_bps)
            {
                return Err(invalid(
                    "请检查网格范围、间距、初始股数及递增比例（-100% 至 1000%）",
                ));
            }
        }
        Ok(())
    }

    pub fn project(&self, observation: Option<&Observation>) -> GridProjection {
        self.project_with_dividend(observation.and_then(positive_ttm_dividend))
    }

    fn project_with_dividend(&self, dividend: Option<Decimal>) -> GridProjection {
        let mut levels: Vec<_> = self.levels.iter().collect();
        levels.sort_unstable_by_key(|level| level.yield_bps);
        let prices: Option<Vec<_>> = dividend.and_then(|d| {
            levels
                .iter()
                .map(|level| {
                    d.checked_div(Decimal::from(level.yield_bps))?
                        .checked_mul(Decimal::from(10_000))
                        // Bound subsequent money operations even for malformed external data.
                        .filter(|p| {
                            *p > Decimal::ZERO && *p <= Decimal::from(1_000_000_000_000_u64)
                        })
                })
                .collect()
        });
        let budget = Decimal::from_str_exact(&self.budget).unwrap_or_default();
        let mut invested = Decimal::ZERO;
        let mut bought = 0_u64;
        let mut scenarios = Vec::new();
        if let Some(prices) = &prices {
            for (index, level) in levels.iter().enumerate() {
                invested += prices[index] * Decimal::from(level.buy_shares);
                bought += u64::from(level.buy_shares);
                if bought == 0 {
                    continue;
                }
                let mut remaining = bought;
                let mut proceeds = Decimal::ZERO;
                let mut unfilled = 0_u64;
                for sell_index in (0..index).rev() {
                    let requested = u64::from(levels[sell_index].sell_shares);
                    let units = requested.min(remaining);
                    proceeds += prices[sell_index] * Decimal::from(units);
                    remaining -= units;
                    unfilled += requested - units;
                }
                // Unsold shares are valued at the rebound endpoint, never auto-liquidated.
                let profit = proceeds + prices[0] * Decimal::from(remaining) - invested;
                scenarios.push(GridScenario {
                    deepest_yield_bps: level.yield_bps,
                    invested_amount: money(invested),
                    budget_gap: money(budget - invested),
                    sold_shares: bought - remaining,
                    remaining_shares: remaining,
                    unfilled_sell_shares: unfilled,
                    sale_proceeds: money(proceeds),
                    profit: money(profit),
                    budget_return_pct: percent(profit / budget),
                    invested_return_pct: percent(profit / invested),
                });
            }
        }
        GridProjection {
            rebound_yield_bps: levels.first().map(|level| level.yield_bps),
            levels: levels
                .iter()
                .enumerate()
                .map(|(index, level)| GridPrice {
                    yield_bps: level.yield_bps,
                    price: prices.as_ref().map(|p| format!("{:.4}", p[index])),
                    buy_amount: prices
                        .as_ref()
                        .map(|p| money(p[index] * Decimal::from(level.buy_shares))),
                    sell_amount: prices
                        .as_ref()
                        .map(|p| money(p[index] * Decimal::from(level.sell_shares))),
                })
                .collect(),
            scenarios,
        }
    }
}

fn money(value: Decimal) -> String {
    format!("{:.2}", value.round_dp(2))
}
fn percent(value: Decimal) -> String {
    format!("{:.4}", (value * Decimal::ONE_HUNDRED).round_dp(4))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridProjection {
    pub rebound_yield_bps: Option<u16>,
    pub levels: Vec<GridPrice>,
    pub scenarios: Vec<GridScenario>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridPrice {
    pub yield_bps: u16,
    pub price: Option<String>,
    pub buy_amount: Option<String>,
    pub sell_amount: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridScenario {
    pub deepest_yield_bps: u16,
    pub invested_amount: String,
    pub budget_gap: String,
    pub sold_shares: u64,
    pub remaining_shares: u64,
    pub unfilled_sell_shares: u64,
    pub sale_proceeds: String,
    pub profit: String,
    pub budget_return_pct: String,
    pub invested_return_pct: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan() -> GridPlan {
        GridPlan {
            budget: "10000".into(),
            generator: None,
            levels: vec![
                GridLevel {
                    yield_bps: 400,
                    buy_shares: 0,
                    sell_shares: 50,
                },
                GridLevel {
                    yield_bps: 500,
                    buy_shares: 100,
                    sell_shares: 50,
                },
                GridLevel {
                    yield_bps: 625,
                    buy_shares: 200,
                    sell_shares: 0,
                },
            ],
        }
    }
    #[test]
    fn computes_explicit_shares_budget_and_unsold_value_without_forced_exit() {
        let plan = plan();
        plan.validate().unwrap();
        let result = plan.project_with_dividend(Some(Decimal::from(5)));
        assert_eq!(result.levels[2].buy_amount.as_deref(), Some("16000.00"));
        let partial = &result.scenarios[0];
        assert_eq!(partial.invested_amount, "10000.00");
        assert_eq!(partial.sold_shares, 50);
        assert_eq!(partial.remaining_shares, 50);
        assert_eq!(partial.profit, "2500.00");
        let full = &result.scenarios[1];
        assert_eq!(full.invested_amount, "26000.00");
        assert_eq!(full.budget_gap, "-16000.00");
        assert_eq!(full.sold_shares, 100);
        assert_eq!(full.remaining_shares, 200);
        assert_eq!(full.sale_proceeds, "11250.00");
        assert_eq!(full.profit, "10250.00");
        assert_eq!(full.budget_return_pct, "102.5000");
        assert_eq!(full.invested_return_pct, "39.4231");
    }
    #[test]
    fn reports_sell_shortfall_and_keeps_quantities_without_dividend() {
        let mut plan = plan();
        plan.levels[0].sell_shares = 500;
        let result = plan.project_with_dividend(Some(Decimal::from(5)));
        assert_eq!(result.scenarios[0].unfilled_sell_shares, 400);
        assert_eq!(result.scenarios[0].remaining_shares, 0);
        let missing = plan.project(None);
        assert!(missing.scenarios.is_empty());
        assert_eq!(missing.levels.len(), 3);
        assert!(missing
            .levels
            .iter()
            .all(|p| p.price.is_none() && p.buy_amount.is_none()));
    }
    #[test]
    fn deeper_touch_without_new_buy_can_cross_another_sell_level() {
        let mut p = plan();
        p.levels.push(GridLevel {
            yield_bps: 700,
            buy_shares: 0,
            sell_shares: 0,
        });
        p.levels[2].sell_shares = 25;
        let result = p.project_with_dividend(Some(Decimal::from(5)));
        assert_eq!(result.scenarios.len(), 3);
        let deepest = result.scenarios.last().unwrap();
        assert_eq!(deepest.invested_amount, "26000.00");
        assert_eq!(deepest.sold_shares, 125);
        assert_eq!(deepest.remaining_shares, 175);
        assert_eq!(deepest.sale_proceeds, "13250.00");
    }

    #[test]
    fn validates_budget_shares_and_custom_yield_ranges() {
        let mut p = plan();
        p.levels[2].yield_bps = 731;
        assert!(p.validate().is_ok());
        for budget in ["0", "-1", "NaN", "1.001", "1000000000001"] {
            let mut invalid = p.clone();
            invalid.budget = budget.into();
            assert!(invalid.validate().is_err());
        }
        p.levels[2].yield_bps = 500;
        assert!(p.validate().is_err());
        p.levels[2].yield_bps = 0;
        assert!(p.validate().is_err());
        assert!(serde_json::from_str::<GridLevel>(
            r#"{"yieldBps":500,"buyShares":1.5,"sellShares":0}"#
        )
        .is_err());
        let mut p = plan();
        p.generator = Some(GridGenerator {
            start_yield_bps: 400,
            end_yield_bps: 600,
            step_bps: 30,
            initial_buy_shares: 100,
            initial_sell_shares: 50,
            buy_growth_bps: 0,
            sell_growth_bps: 0,
        });
        assert!(p.validate().is_err());
    }
}
