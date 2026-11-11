use crate::{AppError, AppResult};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const LOT: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SizeUnit {
    Amount,
    BudgetPct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridLevel {
    pub price: String,
    pub size: String,
}

/// Levels below the base price buy and levels above it sell; sizes become whole lots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridPlan {
    pub budget: String,
    pub base_price: String,
    pub size_unit: SizeUnit,
    pub levels: Vec<GridLevel>,
}

fn decimal(value: &str, max: u64) -> Option<Decimal> {
    Decimal::from_str_exact(value)
        .ok()
        .filter(|v| *v >= Decimal::ZERO && *v <= Decimal::from(max) && v.scale() <= 2)
}
fn price(value: &str) -> Option<Decimal> {
    decimal(value, 1_000_000).filter(|v| *v > Decimal::ZERO)
}

impl GridPlan {
    pub fn validate(&self) -> AppResult<()> {
        let invalid = |message: &str| AppError::Validation(message.into());
        if decimal(&self.budget, 1_000_000_000_000).is_none_or(|b| b.is_zero()) {
            return Err(invalid(
                "请填写大于 0、最多两位小数且不超过一万亿元的配置金额",
            ));
        }
        let base = price(&self.base_price)
            .ok_or_else(|| invalid("基准价须为大于 0、最多两位小数且不超过 100 万元的价格"))?;
        if self.levels.is_empty() || self.levels.len() > 100 {
            return Err(invalid("请设置 1 至 100 个网格档位"));
        }
        let max_size = match self.size_unit {
            SizeUnit::Amount => 1_000_000_000_000,
            SizeUnit::BudgetPct => 1_000,
        };
        let mut prices = BTreeSet::new();
        let mut buys = false;
        for level in &self.levels {
            let p = price(&level.price)
                .filter(|p| *p != base && prices.insert(*p))
                .ok_or_else(|| {
                    invalid("档位价格须为大于 0、最多两位小数、互不重复且不等于基准价的价格")
                })?;
            let size = decimal(&level.size, max_size).ok_or_else(|| {
                invalid("每档规模须为最多两位小数的非负数，金额不超过一万亿元、比例不超过 1000%")
            })?;
            buys |= p < base && size > Decimal::ZERO;
        }
        if !buys {
            return Err(invalid("至少在基准价以下设置一档买入规模"));
        }
        Ok(())
    }

    /// Callers validate first; the projection never trims sizes to fit the budget.
    pub fn project(&self) -> GridProjection {
        let budget = Decimal::from_str_exact(&self.budget).unwrap_or_default();
        let base = Decimal::from_str_exact(&self.base_price).unwrap_or_default();
        let mut levels: Vec<_> = self
            .levels
            .iter()
            .map(|level| {
                let price = Decimal::from_str_exact(&level.price).unwrap_or_default();
                let size = Decimal::from_str_exact(&level.size).unwrap_or_default();
                let target = match self.size_unit {
                    SizeUnit::Amount => size,
                    SizeUnit::BudgetPct => budget * size / Decimal::ONE_HUNDRED,
                };
                let lots = (target / price / Decimal::from(LOT)).floor();
                (price, target, lots.to_u64().unwrap_or(0) * LOT)
            })
            .collect();
        levels.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        let buys: Vec<_> = levels.iter().filter(|l| l.0 < base).collect();
        let mut sells: Vec<_> = levels.iter().filter(|l| l.0 > base).collect();
        sells.reverse();
        let top = sells.last().map(|l| l.0);

        let mut invested = Decimal::ZERO;
        let mut bought = 0_u64;
        let mut cumulative = Vec::new();
        let mut scenarios = Vec::new();
        for &&(price, _, shares) in &buys {
            invested += price * Decimal::from(shares);
            bought += shares;
            cumulative.push((price, invested));
            if bought == 0 {
                continue;
            }
            let held = Decimal::from(bought);
            let base_profit = base * held - invested;
            let mut remaining = bought;
            let mut proceeds = Decimal::ZERO;
            for &&(sell_price, _, requested) in &sells {
                let units = requested.min(remaining);
                proceeds += sell_price * Decimal::from(units);
                remaining -= units;
            }
            // Unsold shares are valued at the highest sell level, never auto-liquidated.
            let top_profit = top.map(|t| proceeds + t * Decimal::from(remaining) - invested);
            scenarios.push(GridScenario {
                deepest_price: money(price),
                invested_amount: money(invested),
                budget_pct: percent(invested / budget),
                bought_shares: bought,
                average_cost: format!("{:.4}", (invested / held).round_dp(4)),
                base_profit: money(base_profit),
                base_return_pct: percent(base_profit / invested),
                sold_shares: bought - remaining,
                remaining_shares: remaining,
                sale_proceeds: money(proceeds),
                top_profit: top_profit.map(money),
                top_return_pct: top_profit.map(|p| percent(p / invested)),
            });
        }
        GridProjection {
            top_price: top.map(money),
            levels: levels
                .iter()
                .map(|&(price, target, shares)| {
                    let reached = cumulative.iter().find(|c| c.0 == price).map(|c| c.1);
                    GridPrice {
                        price: money(price),
                        side: if price < base {
                            GridSide::Buy
                        } else {
                            GridSide::Sell
                        },
                        target_amount: money(target),
                        shares,
                        amount: money(price * Decimal::from(shares)),
                        cumulative_amount: reached.map(money),
                        budget_pct: reached.map(|c| percent(c / budget)),
                    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GridSide {
    Buy,
    Sell,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridProjection {
    pub top_price: Option<String>,
    pub levels: Vec<GridPrice>,
    pub scenarios: Vec<GridScenario>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridPrice {
    pub price: String,
    pub side: GridSide,
    pub target_amount: String,
    pub shares: u64,
    pub amount: String,
    pub cumulative_amount: Option<String>,
    pub budget_pct: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridScenario {
    pub deepest_price: String,
    pub invested_amount: String,
    pub budget_pct: String,
    pub bought_shares: u64,
    pub average_cost: String,
    pub base_profit: String,
    pub base_return_pct: String,
    pub sold_shares: u64,
    pub remaining_shares: u64,
    pub sale_proceeds: String,
    pub top_profit: Option<String>,
    pub top_return_pct: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn level(price: &str, size: &str) -> GridLevel {
        GridLevel {
            price: price.into(),
            size: size.into(),
        }
    }
    fn plan() -> GridPlan {
        GridPlan {
            budget: "100000".into(),
            base_price: "10".into(),
            size_unit: SizeUnit::Amount,
            levels: vec![
                level("11", "5500"),
                level("9", "9000"),
                level("8", "20050"),
                level("12", "100000"),
            ],
        }
    }

    #[test]
    fn converts_sizes_to_whole_lots_and_tracks_budget_share_while_falling() {
        let plan = plan();
        plan.validate().unwrap();
        let result = plan.project();
        let prices: Vec<_> = result.levels.iter().map(|l| l.price.as_str()).collect();
        assert_eq!(prices, ["12.00", "11.00", "9.00", "8.00"]);
        let deepest = &result.levels[3];
        assert_eq!(deepest.side, GridSide::Buy);
        assert_eq!(deepest.shares, 2500);
        assert_eq!(deepest.amount, "20000.00");
        assert_eq!(deepest.cumulative_amount.as_deref(), Some("29000.00"));
        assert_eq!(deepest.budget_pct.as_deref(), Some("29.0000"));
        assert_eq!(result.levels[0].side, GridSide::Sell);
        assert!(result.levels[0].cumulative_amount.is_none());
        assert_eq!(result.top_price.as_deref(), Some("12.00"));
    }

    #[test]
    fn sells_only_bought_shares_on_the_way_up_and_values_the_rest_at_the_top() {
        let result = plan().project();
        let first = &result.scenarios[0];
        assert_eq!(first.deepest_price, "9.00");
        assert_eq!(first.bought_shares, 1000);
        assert_eq!(first.base_profit, "1000.00");
        assert_eq!(first.base_return_pct, "11.1111");
        assert_eq!(first.sold_shares, 1000);
        assert_eq!(first.remaining_shares, 0);
        assert_eq!(first.sale_proceeds, "11500.00");
        assert_eq!(first.top_profit.as_deref(), Some("2500.00"));
        let deepest = &result.scenarios[1];
        assert_eq!(deepest.invested_amount, "29000.00");
        assert_eq!(deepest.average_cost, "8.2857");
        assert_eq!(deepest.sold_shares, 3500);
        assert_eq!(deepest.sale_proceeds, "41500.00");
        assert_eq!(deepest.top_profit.as_deref(), Some("12500.00"));
    }

    #[test]
    fn budget_percent_sizes_without_sell_levels_only_report_the_base_rebound() {
        let mut p = plan();
        p.size_unit = SizeUnit::BudgetPct;
        p.levels = vec![level("9", "9"), level("8", "120")];
        p.validate().unwrap();
        let result = p.project();
        assert!(result.top_price.is_none());
        let deepest = result.scenarios.last().unwrap();
        assert_eq!(deepest.invested_amount, "129000.00");
        assert_eq!(deepest.budget_pct, "129.0000");
        assert!(deepest.top_profit.is_none());
    }

    #[test]
    fn lots_smaller_than_one_hand_are_kept_but_do_not_create_scenarios() {
        let mut p = plan();
        p.levels = vec![level("9", "800"), level("8", "1600")];
        let result = p.project();
        assert_eq!(result.levels[0].shares, 0);
        assert_eq!(result.scenarios.len(), 1);
        assert_eq!(result.scenarios[0].deepest_price, "8.00");
    }

    #[test]
    fn rejects_invalid_budget_prices_sizes_and_missing_buys() {
        assert!(plan().validate().is_ok());
        for budget in ["0", "-1", "NaN", "1.001", "1000000000001"] {
            let mut p = plan();
            p.budget = budget.into();
            assert!(p.validate().is_err());
        }
        for (price, size) in [
            ("10", "1"),
            ("9", "1"),
            ("0", "1"),
            ("8.001", "1"),
            ("7", "-1"),
        ] {
            let mut p = plan();
            p.levels.push(level(price, size));
            assert!(p.validate().is_err(), "{price} {size}");
        }
        let mut p = plan();
        p.size_unit = SizeUnit::BudgetPct;
        assert!(p.validate().is_err());
        let mut p = plan();
        p.levels = vec![level("9", "0"), level("11", "100")];
        assert!(p.validate().is_err());
        assert!(
            serde_json::from_str::<GridLevel>(r#"{"price":"9","size":"1","yieldBps":1}"#).is_err()
        );
    }
}
