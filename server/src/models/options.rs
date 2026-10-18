//! Option contracts belong to one account's underlying stock; premiums are per underlying unit.
use super::{HoldingTrade, HoldingTradeSummary, TradeKind};
use crate::{AppError, AppResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionContract {
    pub underlying_id: String,
    pub account: String,
    pub option_type: OptionType,
    pub side: OptionSide,
    pub strike: String,
    pub expires_on: String,
    pub multiplier: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionType {
    Call,
    Put,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionSide {
    Long,
    Short,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockOptionTotals {
    pub market_value: String,
    pub cost_basis: String,
    pub unrealized_profit: Option<String>,
    pub settled_profit: Option<String>,
    pub option_settled_profit: String,
    pub option_unrealized_profit: String,
    pub usable: bool,
    pub open_contracts: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionPriceInput {
    pub request_id: String,
    pub expected_revision: String,
    pub price: String,
    pub observed_on: String,
}
fn invalid(message: &str) -> AppError {
    AppError::Validation(message.into())
}
pub(crate) fn text(value: Decimal) -> String {
    value.normalize().to_string()
}
pub(crate) fn checked(value: Option<Decimal>) -> AppResult<Decimal> {
    value
        .filter(|n| n.abs() <= Decimal::from(1_000_000_000_000_000_i64))
        .ok_or_else(|| invalid("期权计算超过允许范围"))
}
impl OptionContract {
    pub(crate) fn validate(&self) -> AppResult<()> {
        uuid::Uuid::parse_str(&self.underlying_id).map_err(|_| invalid("期权标的持仓编号无效"))?;
        let date = chrono::NaiveDate::parse_from_str(&self.expires_on, "%Y-%m-%d")
            .map_err(|_| invalid("期权到期日无效"))?;
        let strike = super::decimal::parse(&self.strike)?;
        let multiplier = super::decimal::parse(&self.multiplier)?;
        if date.to_string() != self.expires_on
            || strike <= Decimal::ZERO
            || multiplier <= Decimal::ZERO
            || multiplier.fract() != Decimal::ZERO
            || multiplier > Decimal::from(1_000_000)
            || self.account.trim().is_empty()
            || self.account != self.account.trim()
            || self.account.chars().count() > 120
        {
            return Err(invalid("请核对期权账户、行权价、到期日和整数合约乘数"));
        }
        Ok(())
    }
    pub(crate) fn signed_multiplier(&self) -> AppResult<Decimal> {
        let value = super::decimal::parse(&self.multiplier)?;
        Ok(if self.side == OptionSide::Short {
            -value
        } else {
            value
        })
    }
    pub(crate) fn value(&self, quantity: Decimal, price: Decimal) -> AppResult<Decimal> {
        let multiplier = self.signed_multiplier()?;
        checked(
            quantity
                .checked_mul(price)
                .and_then(|v| v.checked_mul(multiplier)),
        )
    }
}

/// Common execution shape/date/amount checks are performed by the transaction repository first.
pub(crate) fn position(
    contract: &OptionContract,
    trades: &[HoldingTrade],
) -> AppResult<HoldingTradeSummary> {
    contract.validate()?;
    let mut ordered = trades.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|t| (&t.occurred_on, t.sequence));
    let mut ids = HashSet::new();
    let mut sequences = HashSet::new();
    let mut receipts = HashSet::new();
    let mut quantity = Decimal::ZERO;
    // Signed basis: money paid for a long; net opening proceeds owed by a short.
    let mut basis = Decimal::ZERO;
    let mut realized = Decimal::ZERO;
    let mut cash = Decimal::ZERO;
    let mut fees = Decimal::ZERO;
    let mut taxes = Decimal::ZERO;
    let mut expired = false;
    let mut result = HoldingTradeSummary::default();
    let multiplier = super::decimal::parse(&contract.multiplier)?;
    for t in ordered {
        let first = trades.first().unwrap();
        if t.holding_id != first.holding_id
            || t.currency != first.currency
            || t.account != contract.account
            || !ids.insert(&t.id)
            || !sequences.insert((&t.occurred_on, t.sequence))
            || (!t.external_id.is_empty() && !receipts.insert(&t.external_id))
        {
            return Err(invalid("期权成交的账户、币种、编号或同日顺序无效"));
        }
        let q = super::decimal::parse(&t.quantity)?;
        let gross = super::decimal::parse(&t.gross_amount)?;
        let fee = super::decimal::parse(&t.fees)?;
        let tax = super::decimal::parse(&t.taxes)?;
        if q <= Decimal::ZERO || !q.fract().is_zero() || t.kind == TradeKind::Dividend || expired {
            return Err(invalid("期权须填写正整数张数，不支持分红或到期后的成交"));
        }
        if t.kind == TradeKind::Expire {
            if t.occurred_on != contract.expires_on
                || q != quantity
                || !gross.is_zero()
                || !super::decimal::parse(&t.price)?.is_zero()
            {
                return Err(invalid(
                    "到期作废须在到期日一次关闭剩余合约，权利金和成交额为零",
                ));
            }
            expired = true;
        } else if t.occurred_on > contract.expires_on {
            return Err(invalid("期权成交日期不能晚于到期日"));
        }
        let opening = matches!(
            (contract.side, t.kind),
            (OptionSide::Long, TradeKind::Buy) | (OptionSide::Short, TradeKind::Sell)
        );
        let flow = match t.kind {
            TradeKind::Buy => -(gross + fee + tax),
            TradeKind::Sell => gross - fee - tax,
            TradeKind::Expire => -(fee + tax),
            TradeKind::Dividend => unreachable!(),
        };
        if opening {
            quantity = checked(quantity.checked_add(q))?;
            basis = checked(basis.checked_sub(flow))?;
        } else {
            if q > quantity {
                return Err(invalid("期权平仓超过当时未平仓张数，请先补齐开仓记录"));
            }
            let closed_basis = if q == quantity {
                basis
            } else {
                checked(basis.checked_mul(q).and_then(|v| v.checked_div(quantity)))?
            };
            let profit = checked(flow.checked_sub(closed_basis))?;
            realized = checked(realized.checked_add(profit))?;
            result.profits.insert(t.id.clone(), text(profit));
            quantity -= q;
            basis = if quantity.is_zero() {
                Decimal::ZERO
            } else {
                checked(basis.checked_sub(closed_basis))?
            };
        }
        cash = checked(cash.checked_add(flow))?;
        fees = checked(fees.checked_add(fee))?;
        taxes = checked(taxes.checked_add(tax))?;
        result.cash_flows.insert(t.id.clone(), text(flow));
    }
    result.account = contract.account.clone();
    result.quantity = text(quantity);
    result.cost_basis = text(basis);
    result.average_cost = text(if quantity.is_zero() {
        Decimal::ZERO
    } else {
        checked(
            basis
                .abs()
                .checked_div(quantity)
                .and_then(|v| v.checked_div(multiplier)),
        )?
    });
    result.realized_profit = text(realized);
    result.settled_profit = text(realized);
    result.dividend_income = "0".into();
    result.total_fees = text(fees);
    result.total_taxes = text(taxes);
    result.net_cash_flow = text(cash);
    Ok(result)
}
