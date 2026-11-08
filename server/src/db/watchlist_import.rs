//! External v1 watchlist exports are converted once, then use the normal atomic restore.
use super::{
    AppError, AppResult, Database, SyncDataset, SyncTable, SyncValue, SYNC_DATASET_SCHEMA_VERSION,
    SYNC_TABLES,
};
use crate::models::{accounting::HoldingAmounts, watch::WatchInput, HoldingTrade, TradeKind};
use chrono::{DateTime, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Export {
    version: String,
    exported_at: String,
    selected_account_id: Option<String>,
    data: Data,
}
#[derive(Deserialize)]
struct Data {
    accounts: Option<Vec<Account>>,
    watchlist: Option<Vec<Stock>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    id: String,
    name: String,
    watchlist: Vec<Stock>,
    fee_config: Option<Value>,
    cash_tracking_enabled: Option<bool>,
    cash_balance: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stock {
    code: String,
    name: String,
    price: Option<Value>,
    shares: Option<Value>,
    cost_price: Option<Value>,
    #[serde(default)]
    transactions: Vec<Execution>,
}
#[derive(Deserialize)]
struct Execution {
    #[serde(rename = "type")]
    kind: String,
    qty: Value,
    price: Value,
    ts: i64,
    gross: Option<Value>,
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::Validation(message.into())
}
// Normalize only sub-picounit JSON float artifacts, never genuine ninth-decimal amounts.
fn decimal(value: &Value) -> AppResult<Decimal> {
    let raw = match value {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => return Err(invalid("导入数量或金额必须为数字或十进制字符串")),
    };
    let n = Decimal::from_str_exact(&raw)
        .or_else(|_| Decimal::from_scientific(&raw))
        .map_err(|_| invalid("导入数量或金额格式无效"))?;
    let rounded = n.round_dp(8);
    if n < Decimal::ZERO
        || n > Decimal::from(1_000_000_000_000_000_i64)
        || (n - rounded).abs() > Decimal::new(1, 12)
    {
        return Err(invalid("导入数量或金额超出范围或超过 8 位有效小数"));
    }
    Ok(rounded.normalize())
}
fn text(n: Decimal) -> String {
    n.normalize().to_string()
}
fn product(a: Decimal, b: Decimal) -> AppResult<Decimal> {
    a.checked_mul(b)
        .filter(|n| n.scale() <= 8 && *n <= Decimal::from(1_000_000_000_000_000_i64))
        .ok_or_else(|| invalid("导入金额乘积超出范围或超过 8 位小数"))
}
fn push(dataset: &mut SyncDataset, name: &str, fields: Value) {
    let table = dataset.tables.iter_mut().find(|t| t.name == name).unwrap();
    table.rows.push(
        table
            .columns
            .iter()
            .map(|c| match &fields[c] {
                Value::String(s) => SyncValue::Text(s.clone()),
                Value::Number(n) => n
                    .as_i64()
                    .map(SyncValue::Integer)
                    .unwrap_or_else(|| SyncValue::Real(n.as_f64().unwrap())),
                Value::Null => SyncValue::Null,
                _ => unreachable!("sync columns are scalars"),
            })
            .collect(),
    );
}

impl Database {
    pub fn import_account_data(&self, data: &Value) -> AppResult<()> {
        let dataset = if data.get("version").is_some() {
            let mut dataset = convert(data)?;
            // A source stock's target yield is not an account watch-rule change.
            push(
                &mut dataset,
                "settings",
                json!({
                    "key":"watch.rules", "value":serde_json::to_string(&self.watch_rules()?)?
                }),
            );
            dataset
        } else {
            serde_json::from_value(data.clone())
                .map_err(|e| invalid(format!("备份格式无效：{e}")))?
        };
        self.import_sync_data(&dataset)
    }
}

fn convert(value: &Value) -> AppResult<SyncDataset> {
    let export: Export = serde_json::from_value(value.clone())
        .map_err(|e| invalid(format!("自选 JSON 格式无效：{e}")))?;
    if export.version != "1.0" {
        return Err(invalid("仅支持 version 为 1.0 的自选 JSON"));
    }
    DateTime::parse_from_rfc3339(&export.exported_at).map_err(|_| invalid("导出时间格式无效"))?;
    let account = if let Some(accounts) = export.data.accounts {
        let mut ids = HashSet::new();
        if accounts.is_empty()
            || accounts
                .iter()
                .any(|a| a.id.trim().is_empty() || !ids.insert(a.id.clone()))
        {
            return Err(invalid("导入账户为空或账户编号重复"));
        }
        if accounts.len() > 1 && export.selected_account_id.is_none() {
            return Err(invalid("JSON 包含多个账户，请先选择要导入的账户"));
        }
        accounts
            .into_iter()
            .find(|a| {
                export
                    .selected_account_id
                    .as_ref()
                    .is_none_or(|id| *id == a.id)
            })
            .ok_or_else(|| invalid("所选导入账户不存在"))?
    } else {
        if export.selected_account_id.is_some() {
            return Err(invalid("所选导入账户不存在"));
        }
        Account {
            id: "default".into(),
            name: "导入账户".into(),
            watchlist: export
                .data
                .watchlist
                .ok_or_else(|| invalid("JSON 缺少自选列表"))?,
            fee_config: None,
            cash_tracking_enabled: None,
            cash_balance: None,
        }
    };
    if account
        .fee_config
        .as_ref()
        .and_then(|f| f.get("enabled"))
        .is_some_and(|v| v != &Value::Bool(false))
    {
        return Err(invalid(
            "来源启用了手续费估算，缺少逐笔实际费用，不能无损导入；请先核对来源成交单",
        ));
    }
    let name = account.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(invalid("导入账户名称须为 1 至 120 字"));
    }
    if account.watchlist.len() > crate::models::watch::MAX_WATCHES {
        return Err(invalid("自选列表超过 50 家上限"));
    }
    let now = Utc::now().to_rfc3339();
    let mut dataset = SyncDataset {
        schema_version: SYNC_DATASET_SCHEMA_VERSION,
        exported_at: export.exported_at,
        tables: SYNC_TABLES
            .iter()
            .map(|s| SyncTable {
                name: s.name.into(),
                columns: s.columns.iter().map(|c| (*c).into()).collect(),
                rows: vec![],
            })
            .collect(),
    };
    let mut symbols = HashSet::new();
    for stock in account.watchlist {
        let symbol = if stock.code.len() == 6 {
            format!(
                "{}.{}",
                stock.code,
                if stock.code.starts_with('6') {
                    "SS"
                } else {
                    "SZ"
                }
            )
        } else {
            stock.code.clone()
        };
        let input = WatchInput {
            symbol,
            name: stock.name.clone(),
            thesis: String::new(),
        }
        .normalize()?;
        if !symbols.insert(input.symbol.clone()) {
            return Err(invalid(format!("重复证券 {}", input.symbol)));
        }
        push(
            &mut dataset,
            "watch_entries",
            json!({"symbol":input.symbol,"payload":json!({"symbol":input.symbol,"name":input.name,"thesis":"","createdAt":now,"pinned":false}).to_string(),"refresh_failed":0}),
        );
        import_stock(&mut dataset, &stock, &input.symbol, name, &now)
            .map_err(|e| invalid(format!("{}（{}）：{e}", stock.name, stock.code)))?;
    }
    if account.cash_tracking_enabled == Some(true) {
        let balances = account
            .cash_balance
            .and_then(|v| v.as_object().cloned())
            .ok_or_else(|| invalid("已启用现金追踪但缺少 cashBalance"))?;
        for (currency, value) in balances {
            if !super::cash::CURRENCIES.contains(&currency.as_str()) {
                return Err(invalid("现金仅支持 CNY、HKD、USD"));
            }
            let amount = decimal(&value)?;
            let id = Uuid::new_v4().to_string();
            push_holding(
                &mut dataset,
                &id,
                "",
                &format!("{currency} 存款"),
                "现金",
                &currency,
                amount,
                Decimal::ZERO,
                None,
                None,
                None,
                &now,
            );
            push(
                &mut dataset,
                "cash_state",
                json!({"currency":currency,"holding_id":id,"amount":text(amount),"baseline_id":Uuid::new_v4().to_string(),"confirmed_on":crate::models::market::today(),"confirmed_at":now,"needs_reconciliation":1}),
            );
        }
    }
    dataset.validate()?;
    Ok(dataset)
}

fn import_stock(
    dataset: &mut SyncDataset,
    stock: &Stock,
    symbol: &str,
    account: &str,
    now: &str,
) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    let mut ordered: Vec<_> = stock.transactions.iter().enumerate().collect();
    if ordered.len() > 50_000 {
        return Err(invalid("交易记录超过 50000 条上限"));
    }
    ordered.sort_by_key(|(index, t)| (t.ts, *index));
    let mut trades = vec![];
    let mut sales: HashMap<(i64, String), Vec<usize>> = HashMap::new();
    for (index, t) in &ordered {
        if t.kind == "sell" {
            sales
                .entry((t.ts, text(decimal(&t.qty)?)))
                .or_default()
                .push(*index);
        }
    }
    let mut taxes_by_sale: HashMap<usize, Vec<&Execution>> = HashMap::new();
    for (_, t) in &ordered {
        if t.kind != "dividendTax" {
            continue;
        }
        let matches = sales
            .get(&(t.ts, text(decimal(&t.qty)?)))
            .filter(|indices| indices.len() == 1)
            .ok_or_else(|| invalid("红利税无法唯一匹配卖出记录"))?;
        taxes_by_sale.entry(matches[0]).or_default().push(t);
    }
    for (index, t) in &ordered {
        if t.kind == "dividendTax" {
            continue;
        }
        let kind = match t.kind.as_str() {
            "buy" => TradeKind::Buy,
            "sell" => TradeKind::Sell,
            "dividend" => TradeKind::Dividend,
            _ => return Err(invalid(format!("不支持的交易类型 {}", t.kind))),
        };
        let qty = decimal(&t.qty)?;
        let price = decimal(&t.price)?;
        if qty <= Decimal::ZERO || !qty.fract().is_zero() {
            return Err(invalid("A 股交易及分红股数必须为正整数"));
        }
        let mut taxes = Decimal::ZERO;
        let gross = if kind == TradeKind::Dividend {
            let per_share = t.gross.as_ref().map(decimal).transpose()?.unwrap_or(price);
            if per_share < price {
                return Err(invalid("税前每股分红不能小于实收每股分红"));
            }
            taxes = product(qty, per_share - price)?;
            product(qty, per_share)?
        } else {
            product(qty, price)?
        };
        let mut sources = vec![&stock.transactions[*index]];
        for tax in taxes_by_sale.get(index).into_iter().flatten() {
            taxes = taxes
                .checked_add(decimal(&tax.price)?)
                .ok_or_else(|| invalid("税费溢出"))?;
            sources.push(tax);
        }
        let timestamp = DateTime::<Utc>::from_timestamp_millis(t.ts)
            .ok_or_else(|| invalid("交易时间戳无效"))?;
        let occurred_on = timestamp
            .with_timezone(&chrono_tz::Asia::Shanghai)
            .date_naive()
            .to_string();
        let trade = HoldingTrade { id: Uuid::new_v4().to_string(), holding_id: id.clone(), kind, occurred_on,
            sequence: u32::try_from(trades.len() + 1).map_err(|_| invalid("交易数量超限"))?,
            quantity: if kind == TradeKind::Dividend { "0".into() } else { text(qty) },
            price: if kind == TradeKind::Dividend { "0".into() } else { text(price) }, gross_amount: text(gross), fees: "0".into(), taxes: text(taxes), currency: "CNY".into(), account: account.into(), external_id: String::new(),
            note: format!("自选 JSON 导入；源记录：{}", serde_json::to_string(&sources.iter().map(|s| json!({"type":s.kind,"qty":s.qty,"price":s.price,"ts":s.ts,"gross":s.gross})).collect::<Vec<_>>())?),
            created_at: timestamp.to_rfc3339(), updated_at: now.into() };
        trades.push(trade);
    }
    let summary = super::trades::position_with_splits(&trades, &[])?;
    let shares = stock.shares.as_ref().map(decimal).transpose()?;
    let (quantity, basis, average) = if trades.is_empty() {
        let quantity = shares.unwrap_or(Decimal::ZERO);
        if !quantity.fract().is_zero() {
            return Err(invalid("持仓股数必须为整数"));
        }
        if quantity.is_zero() {
            return Ok(());
        }
        let cost = stock
            .cost_price
            .as_ref()
            .ok_or_else(|| invalid("手工持仓缺少成本价"))
            .and_then(decimal)?;
        (quantity, product(quantity, cost)?, cost)
    } else {
        let quantity = Decimal::from_str_exact(&summary.quantity).unwrap();
        if shares.is_some_and(|q| q != quantity) {
            return Err(invalid(
                "源 shares 与逐笔交易重放后的股数不一致，请补齐历史",
            ));
        }
        (
            quantity,
            Decimal::from_str_exact(&summary.cost_basis).unwrap(),
            Decimal::from_str_exact(&summary.average_cost).unwrap(),
        )
    };
    let price = stock.price.as_ref().map(decimal).transpose()?;
    if quantity > Decimal::ZERO && price.is_none_or(|p| p <= Decimal::ZERO) {
        return Err(invalid("非零持仓缺少有效的 price 参考价，请核对来源"));
    }
    let market = product(quantity, price.unwrap_or(Decimal::ZERO))?;
    push_holding(
        dataset,
        &id,
        symbol,
        &stock.name,
        "股票",
        "CNY",
        market,
        basis,
        Some(quantity),
        Some(average),
        price,
        now,
    );
    let amounts = HoldingAmounts {
        price_status: "unconfirmed".into(),
        market_value: text(market),
        cost_basis: text(basis),
        quantity: Some(text(quantity)),
        cost_price: Some(text(average)),
        current_price: price.map(text),
    };
    push(
        dataset,
        "holding_amounts",
        json!({"holding_id":id,"payload":serde_json::to_string(&amounts)?}),
    );
    for trade in trades {
        push(
            dataset,
            "holding_trades",
            json!({"id":trade.id,"holding_id":id,"payload":serde_json::to_string(&trade)?,"created_at":trade.created_at}),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_holding(
    dataset: &mut SyncDataset,
    id: &str,
    symbol: &str,
    name: &str,
    class: &str,
    currency: &str,
    market: Decimal,
    basis: Decimal,
    quantity: Option<Decimal>,
    average: Option<Decimal>,
    price: Option<Decimal>,
    now: &str,
) {
    push(
        dataset,
        "holdings",
        json!({"id":id,"symbol":symbol,"name":name,"asset_class":class,"currency":currency,"market_value":market.to_f64().unwrap(),"cost_basis":basis.to_f64().unwrap(),"quantity":quantity.map(|n|n.to_f64().unwrap()),"cost_price":average.map(|n|n.to_f64().unwrap()),"current_price":price.map(|n|n.to_f64().unwrap()),"target_pct":0,"valuation_date":"","fx_rate_source":"","fx_rate_observed_on":"","created_at":now,"updated_at":now}),
    );
}
