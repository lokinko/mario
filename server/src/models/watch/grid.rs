use super::{positive_ttm_dividend, Observation};
use crate::{AppError, AppResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const EXIT_YIELD: u16 = 400;
const MAX_YIELD: u16 = 600;
const FULL: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GridBuyBasis {
    Budget,
    RemainingCash,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GridSellBasis {
    TotalBought,
    RemainingShares,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridLevel {
    pub yield_bps: u16,
    pub buy_bps: u16,
    pub sell_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridPlan {
    pub buy_basis: GridBuyBasis,
    pub sell_basis: GridSellBasis,
    pub levels: Vec<GridLevel>,
}

impl GridPlan {
    pub fn validate(&self) -> AppResult<()> {
        let invalid = |message: &str| AppError::Validation(message.into());
        let mut yields = BTreeSet::new();
        let mut buy = 0;
        let mut sell = 0;
        for level in &self.levels {
            if !(EXIT_YIELD..=MAX_YIELD).contains(&level.yield_bps)
                || level.yield_bps % 10 != 0
                || !yields.insert(level.yield_bps)
            {
                return Err(invalid("档位须为 4.0% 至 6.0% 内不重复的 0.1% 刻度"));
            }
            if u32::from(level.buy_bps) > FULL || u32::from(level.sell_bps) > FULL {
                return Err(invalid("每档买入和卖出比例须在 0% 至 100% 之间"));
            }
            if [EXIT_YIELD, MAX_YIELD].contains(&level.yield_bps) && level.sell_bps != 0 {
                return Err(invalid("中途减仓档位须高于 4.0% 且低于 6.0%"));
            }
            buy += u32::from(level.buy_bps);
            sell += u32::from(level.sell_bps);
        }
        if buy == 0 {
            return Err(invalid("至少填写一档买入比例"));
        }
        if self.buy_basis == GridBuyBasis::Budget && buy > FULL {
            return Err(invalid("买入比例合计不能超过计划资金的 100%"));
        }
        if self.sell_basis == GridSellBasis::TotalBought && sell > FULL {
            return Err(invalid("中途减仓比例合计不能超过本轮买入股数的 100%"));
        }
        Ok(())
    }

    pub fn project(&self, observation: Option<&Observation>) -> GridProjection {
        let mut levels: Vec<_> = self.levels.iter().collect();
        levels.sort_unstable_by_key(|level| level.yield_bps);
        let dividend = observation.and_then(positive_ttm_dividend);
        let price = |yield_bps: u16| {
            dividend?
                .checked_mul(Decimal::from(FULL))?
                .checked_div(Decimal::from(yield_bps))
                .map(|price| format!("{price:.4}"))
        };
        let mut invested = Decimal::ZERO;
        // Share units are normalized by one fixed dividend. This cancels the
        // dividend in scenario returns and never represents ledger quantities.
        let mut bought = Decimal::ZERO;
        let mut scenarios = Vec::new();
        for (index, level) in levels.iter().enumerate() {
            let available = match self.buy_basis {
                GridBuyBasis::Budget => Decimal::ONE,
                GridBuyBasis::RemainingCash => Decimal::ONE - invested,
            };
            let investment = available * fraction(level.buy_bps);
            invested += investment;
            bought += investment * Decimal::from(level.yield_bps);
            if invested.is_zero() || (level.buy_bps == 0 && level.sell_bps == 0) {
                continue;
            }
            let mut remaining = bought;
            let mut proceeds = Decimal::ZERO;
            for sell in levels[..index].iter().rev() {
                let base = match self.sell_basis {
                    GridSellBasis::TotalBought => bought,
                    GridSellBasis::RemainingShares => remaining,
                };
                let units = (base * fraction(sell.sell_bps)).min(remaining);
                proceeds += units / Decimal::from(sell.yield_bps);
                remaining -= units;
            }
            proceeds += remaining / Decimal::from(EXIT_YIELD);
            let profit = proceeds - invested;
            scenarios.push(GridScenario {
                deepest_yield_bps: level.yield_bps,
                invested_pct: percent(invested),
                exit_shares_pct: percent(remaining / bought),
                budget_return_pct: percent(profit),
                invested_return_pct: percent(profit / invested),
            });
        }
        GridProjection {
            exit_price: price(EXIT_YIELD),
            levels: levels
                .into_iter()
                .map(|level| GridPrice {
                    yield_bps: level.yield_bps,
                    price: price(level.yield_bps),
                })
                .collect(),
            scenarios,
        }
    }
}

fn fraction(bps: u16) -> Decimal {
    Decimal::from(bps) / Decimal::from(FULL)
}

fn percent(value: Decimal) -> String {
    format!("{:.4}", (value * Decimal::ONE_HUNDRED).round_dp(4))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridProjection {
    pub exit_price: Option<String>,
    pub levels: Vec<GridPrice>,
    pub scenarios: Vec<GridScenario>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridPrice {
    pub yield_bps: u16,
    pub price: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridScenario {
    pub deepest_yield_bps: u16,
    pub invested_pct: String,
    pub exit_shares_pct: String,
    pub budget_return_pct: String,
    pub invested_return_pct: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(yield_bps: u16, buy_bps: u16, sell_bps: u16) -> GridLevel {
        GridLevel {
            yield_bps,
            buy_bps,
            sell_bps,
        }
    }

    #[test]
    fn models_partial_and_full_rebounds_with_interim_sales() {
        let plan = GridPlan {
            buy_basis: GridBuyBasis::Budget,
            sell_basis: GridSellBasis::TotalBought,
            levels: vec![
                level(600, 5000, 0),
                level(550, 3000, 0),
                level(500, 2000, 2000),
                level(450, 0, 3000),
            ],
        };
        plan.validate().unwrap();
        let result = plan.project(None);
        assert_eq!(result.exit_price, None);
        assert_eq!(result.scenarios.len(), 3);
        let first = &result.scenarios[0];
        assert_eq!(first.deepest_yield_bps, 500);
        assert_eq!(first.invested_pct, "20.0000");
        assert_eq!(first.exit_shares_pct, "70.0000");
        assert_eq!(first.budget_return_pct, "4.1667");
        assert_eq!(first.invested_return_pct, "20.8333");
        let full = &result.scenarios[2];
        assert_eq!(full.invested_pct, "100.0000");
        assert_eq!(full.exit_shares_pct, "50.0000");
        assert_eq!(full.budget_return_pct, "30.8917");
    }

    #[test]
    fn remaining_cash_and_share_percentages_apply_to_a_shrinking_base() {
        let plan = GridPlan {
            buy_basis: GridBuyBasis::RemainingCash,
            sell_basis: GridSellBasis::RemainingShares,
            levels: vec![
                level(450, 0, 5000),
                level(500, 5000, 5000),
                level(600, 5000, 0),
            ],
        };
        plan.validate().unwrap();
        let result = plan.project(None);
        let full = result.scenarios.last().unwrap();
        assert_eq!(full.invested_pct, "75.0000");
        assert_eq!(full.exit_shares_pct, "25.0000");
        assert_eq!(full.budget_return_pct, "12.2222");
        assert_eq!(full.invested_return_pct, "16.2963");
    }

    #[test]
    fn prevents_overallocation_duplicate_levels_and_overriding_liquidation() {
        for levels in [
            vec![level(500, 5001, 0), level(600, 5000, 0)],
            vec![level(500, 2000, 1000), level(500, 3000, 0)],
            vec![level(400, 2000, 1000)],
            vec![level(500, 1000, 4001), level(550, 1000, 6000)],
            vec![level(500, 0, 5000)],
        ] {
            let plan = GridPlan {
                buy_basis: GridBuyBasis::Budget,
                sell_basis: GridSellBasis::TotalBought,
                levels,
            };
            assert!(plan.validate().is_err());
        }
    }
}
