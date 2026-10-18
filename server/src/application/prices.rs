//! Account price workflows; all network access is supplied through a port.
use crate::{
    db::Database,
    models::{SecurityPriceQuery, Snapshot},
    ports::market::SecurityPriceProvider,
    AppError, AppResult,
};
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub snapshot: Snapshot,
    pub updated_count: usize,
    pub errors: Vec<String>,
}

/// A new position can supply a code and quantity without entering a placeholder price.
pub async fn complete_input(
    mut input: crate::models::HoldingInput,
    provider: &dyn SecurityPriceProvider,
) -> AppResult<crate::models::HoldingInput> {
    if input.asset_class != "现金"
        && !input.symbol.trim().is_empty()
        && input
            .quantity
            .is_some_and(|q| q.is_finite() && q > 0.0 && q <= 1e15)
        && input.current_price.is_none()
    {
        let quote = provider
            .quote(
                &SecurityPriceQuery {
                    action_from: None,
                    symbol: input.symbol.clone(),
                    on_date: crate::models::market::today(),
                },
                "",
            )
            .await?;
        if !input.currency.trim().eq_ignore_ascii_case(&quote.currency) {
            return Err(AppError::Validation(format!(
                "行情币种 {} 与持仓币种 {} 不一致",
                quote.currency, input.currency
            )));
        }
        input.current_price = Some(quote.close);
        let mut exact =
            input
                .exact_values
                .clone()
                .unwrap_or(crate::models::accounting::HoldingAmounts {
                    price_status: "provisional".into(),
                    market_value: input.market_value.to_string(),
                    cost_basis: input.cost_basis.to_string(),
                    quantity: input.quantity.map(|v| v.to_string()),
                    cost_price: input.cost_price.map(|v| v.to_string()),
                    current_price: None,
                });
        exact.current_price = Some(
            quote
                .exact_close
                .clone()
                .unwrap_or_else(|| quote.close.to_string()),
        );
        exact.price_status = "provisional".into();
        input.exact_values = Some(exact);
        input.valuation_date = quote.requested_on;
    }
    Ok(input)
}

pub async fn refresh(
    db: &Database,
    provider: &dyn SecurityPriceProvider,
) -> AppResult<RefreshResult> {
    let snapshot = db.snapshot()?;
    let day = crate::models::market::today();
    let mut errors = Vec::new();
    let mut updated_count = 0;
    let holdings = snapshot
        .holdings
        .iter()
        .filter(|h| {
            h.quantity.is_some_and(|q| q > 0.0)
                && h.asset_class != "现金"
                && !h.symbol.trim().is_empty()
        })
        .collect::<Vec<_>>();
    for chunk in holdings.chunks(4) {
        let fetch = |index: usize| {
            let snapshot = &snapshot;
            let holding = chunk.get(index).copied();
            let day = day.clone();
            async move {
                match holding {
                    Some(holding) => {
                        let query = SecurityPriceQuery {
                            action_from: snapshot
                                .holding_trades
                                .iter()
                                .filter(|t| t.holding_id == holding.id)
                                .map(|t| t.occurred_on.clone())
                                .min(),
                            symbol: holding.symbol.clone(),
                            on_date: day,
                        };
                        Some((holding, provider.quote(&query, "").await))
                    }
                    None => None,
                }
            }
        };
        let (a, b, c, d) = tokio::join!(fetch(0), fetch(1), fetch(2), fetch(3));
        for (holding, quote) in [a, b, c, d].into_iter().flatten() {
            let quantity = holding.quantity.unwrap();
            let result = (|| {
                let quote = quote?;
                if !holding.currency.eq_ignore_ascii_case(&quote.currency) {
                    return Err(AppError::MarketData(format!(
                        "行情币种 {} 与持仓币种 {} 不一致",
                        quote.currency, holding.currency
                    )));
                }
                if db.register_corporate_actions(&holding.id, &quote)? {
                    return Err(AppError::MarketData(
                        "发现待确认拆股或合股，已暂停估值，请按来源核对并确认公司行动".into(),
                    ));
                }
                if snapshot.holding_valuations.iter().any(|v| {
                    v.holding_id == holding.id
                        && v.quantity == quantity
                        && v.unit_price == quote.close
                        && v.requested_on == quote.requested_on
                        && v.observed_on == quote.observed_on
                        && v.provider_code == quote.provider_code
                }) {
                    return Ok(false);
                }
                db.apply_verified_holding_valuation(&holding.id, quantity, &quote)?;
                Ok(true)
            })();
            match result {
                Ok(true) => updated_count += 1,
                Ok(false) => {}
                Err(error) => {
                    errors.push(format!("{}（{}）：{}", holding.name, holding.symbol, error))
                }
            }
        }
    }
    let mut snapshot = db.snapshot()?;
    snapshot.price_refresh_errors = errors.clone();
    Ok(RefreshResult {
        snapshot,
        updated_count,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SecurityPriceQuote;
    use async_trait::async_trait;
    use chrono::NaiveDate;
    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }
    struct FixedProvider {
        price: f64,
        currency: &'static str,
        failed: bool,
    }
    #[async_trait]
    impl SecurityPriceProvider for FixedProvider {
        async fn quote(
            &self,
            query: &SecurityPriceQuery,
            _: &str,
        ) -> AppResult<SecurityPriceQuote> {
            if self.failed {
                return Err(AppError::MarketData("offline".into()));
            }
            let date = day(&query.on_date) - chrono::Duration::days(1);
            Ok(SecurityPriceQuote {
                exact_close: Some(self.price.to_string()),
                splits: vec![],
                symbol: query.symbol.clone(),
                currency: self.currency.into(),
                close: self.price,
                requested_on: query.on_date.clone(),
                observed_on: date.to_string(),
                staleness_days: 1,
                provider_code: "yahoo_daily_close".into(),
                provider_name: "Yahoo Finance".into(),
                exchange: "NMS".into(),
                mic_code: String::new(),
                instrument_type: "EQUITY".into(),
                price_basis: "split_adjusted_daily_close".into(),
                source_url: format!(
                    "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d",
                    query.symbol
                ),
                methodology_url: crate::models::market::METHODOLOGY_URL.into(),
                disclaimer: "fixture".into(),
            })
        }
    }
    fn db() -> Database {
        Database::test()
    }
    fn holding(
        symbol: &str,
        asset_class: &str,
        quantity: Option<f64>,
    ) -> crate::models::HoldingInput {
        serde_json::from_value(serde_json::json!({"symbol":symbol,"name":"测试资产","assetClass":asset_class,"marketValue":100,"costBasis":50,"targetPct":0,"currency":"USD","valuationDate":crate::models::market::today(),"quantity":quantity,"currentPrice":quantity.map(|_|10.0)})).unwrap()
    }
    #[tokio::test]
    async fn refresh_is_idempotent_preserves_manual_assets_and_recovers_without_rewriting_history()
    {
        let db = db();
        let source = FixedProvider {
            price: 23.0,
            currency: "USD",
            failed: false,
        };
        let id = db
            .add_holding(&holding("AAPL", "股票", Some(10.0)))
            .unwrap()
            .holdings[0]
            .id
            .clone();
        db.add_holding(&holding("", "股票", None)).unwrap();
        db.add_holding(&holding("CASH", "现金", Some(10.0)))
            .unwrap();
        let yesterday =
            (day(&crate::models::market::today()) - chrono::Duration::days(1)).to_string();
        {
            let conn = db.conn().unwrap();
            conn.execute(
                "UPDATE daily_entries SET day=$1,id=$1 || ':' || entity_id",
                [&yesterday],
            )
            .unwrap();
            conn.execute("DELETE FROM daily_totals", []).unwrap();
        }
        let result = refresh(&db, &source).await.unwrap();
        assert_eq!(result.updated_count, 1);
        assert!(result.errors.is_empty());
        let updated = result
            .snapshot
            .holdings
            .iter()
            .find(|h| h.id == id)
            .unwrap();
        assert_eq!(
            (
                updated.quantity,
                updated.current_price,
                updated.market_value,
                updated.cost_basis
            ),
            (Some(10.0), Some(23.0), 230.0, 50.0)
        );
        let hash = db.export_sync_data().unwrap().content_hash().unwrap();
        assert_eq!(refresh(&db, &source).await.unwrap().updated_count, 0);
        assert_eq!(db.export_sync_data().unwrap().content_hash().unwrap(), hash);
        for provider in [
            FixedProvider {
                failed: true,
                ..source
            },
            FixedProvider {
                currency: "HKD",
                ..source
            },
        ] {
            let failed = refresh(&db, &provider).await.unwrap();
            assert_eq!(failed.updated_count, 0);
            assert_eq!(failed.errors.len(), 1);
            assert_eq!(failed.snapshot.price_refresh_errors, failed.errors);
            assert_eq!(db.export_sync_data().unwrap().content_hash().unwrap(), hash);
        }
        let restored = self::db();
        restored
            .import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(
            restored.snapshot().unwrap().holding_valuations[0].provider_code,
            "yahoo_daily_close"
        );
        let next = FixedProvider {
            price: 25.0,
            ..source
        };
        assert_eq!(refresh(&db, &next).await.unwrap().updated_count, 1);
        let history = db
            .daily_history(&crate::models::daily::HistoryQuery::default())
            .unwrap();
        let before = history.records.iter().find(|r| r.day == yesterday).unwrap();
        assert_eq!(
            before
                .assets
                .iter()
                .find(|a| a.holding.id == id)
                .unwrap()
                .holding
                .market_value,
            100.0
        );
        let evidence = &db.snapshot().unwrap().holding_valuations[0];
        let mut forged: crate::models::HoldingValuationEvidence = evidence.clone();
        forged.source_url = forged
            .source_url
            .replace("query1.finance.yahoo.com", "attacker.test");
        let mut data = db.export_sync_data().unwrap();
        let valuations = data
            .tables
            .iter_mut()
            .find(|t| t.name == "holding_valuations")
            .unwrap();
        let source_index = valuations
            .columns
            .iter()
            .position(|c| c == "source_url")
            .unwrap();
        valuations.rows[0][source_index] = crate::db::SyncValue::Text(forged.source_url);
        assert!(restored.import_sync_data(&data).is_err());
    }
    #[tokio::test]
    async fn new_positions_can_omit_price_but_currency_errors_do_not_save() {
        let db = db();
        let provider = FixedProvider {
            price: 23.0,
            currency: "USD",
            failed: false,
        };
        let mut input = holding("AAPL", "股票", Some(10.0));
        input.current_price = None;
        let filled = complete_input(input.clone(), &provider).await.unwrap();
        let saved = db.add_holding(&filled).unwrap();
        assert_eq!(saved.holdings[0].market_value, 230.0);
        assert!(complete_input(
            input,
            &FixedProvider {
                currency: "HKD",
                ..provider
            }
        )
        .await
        .is_err());
        assert_eq!(db.snapshot().unwrap().holdings.len(), 1);
    }

    #[tokio::test]
    async fn price_refresh_keeps_trade_quantities_costs_and_execution_records() {
        let db = db();
        let trade = crate::models::HoldingTradeInput {
            expected_cash_baseline: "00000000-0000-0000-0000-000000000001".into(),
            settlement_mode: crate::models::SettlementMode::Settle,
            request_id: uuid::Uuid::new_v4().to_string(),
            holding_id: String::new(),
            expected_revision: None,
            new_holding: Some(crate::models::TradeSecurityInput {
                symbol: "AAPL".into(),
                name: "Apple".into(),
                asset_class: "股票".into(),
                currency: "USD".into(),
            }),
            kind: crate::models::TradeKind::Buy,
            occurred_on: crate::models::market::today(),
            sequence: 1,
            quantity: "10".into(),
            price: "10".into(),
            gross_amount: "100".into(),
            fees: "2".into(),
            taxes: "0".into(),
            account: "broker".into(),
            external_id: String::new(),
            note: String::new(),
        };
        crate::db::fund_cash(&db, "USD");
        let before = db.save_holding_trade(&trade).unwrap();
        let provider = FixedProvider {
            price: 23.0,
            currency: "USD",
            failed: false,
        };
        let updated = refresh(&db, &provider).await.unwrap();
        assert_eq!(updated.updated_count, 1);
        assert_eq!(updated.snapshot.holding_trades, before.holding_trades);
        let holding = &updated.snapshot.holdings[0];
        assert_eq!(holding.quantity, before.holdings[0].quantity);
        assert_eq!(holding.cost_basis, before.holdings[0].cost_basis);
        assert_eq!(holding.cost_price, before.holdings[0].cost_price);
        assert_eq!(holding.market_value, 230.0);
        let restored = self::db();
        restored
            .import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(restored.snapshot().unwrap().holdings[0].market_value, 230.0);
    }
    #[tokio::test]
    #[ignore = "requires live free public API"]
    async fn live_free_quotes() {
        let provider = crate::prices::FreeSecurityPriceProvider::new().unwrap();
        for (symbol, currency) in [("AAPL", "USD"), ("0700.HK", "HKD"), ("600519.SS", "CNY")] {
            let q = provider
                .quote(
                    &SecurityPriceQuery {
                        action_from: None,
                        symbol: symbol.into(),
                        on_date: crate::models::market::today(),
                    },
                    "",
                )
                .await
                .unwrap();
            assert_eq!(q.currency, currency);
            assert!(q.close > 0.0 && q.staleness_days > 0);
            let db = db();
            let mut input = holding(symbol, "股票", Some(10.0));
            input.currency = currency.into();
            db.add_holding(&input).unwrap();
            let applied = refresh(&db, &provider).await.unwrap();
            assert!(applied.errors.is_empty(), "{:?}", applied.errors);
            assert_eq!(applied.updated_count, 1);
            assert_eq!(applied.snapshot.holdings[0].market_value, q.close * 10.0);
            println!(
                "{symbol}: {} {} observed {}",
                q.close, q.currency, q.observed_on
            );
        }
    }
}
