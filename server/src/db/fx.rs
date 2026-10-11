use super::*;
use crate::fx::{FxBook, FxOverrideInput, ManualRate};

fn key(base: &str) -> String {
    format!("fx.book.{base}")
}
fn book_on(conn: &Connection, base: &str) -> AppResult<Option<FxBook>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key=?1",
        [key(base)],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|v| serde_json::from_str(&v).map_err(AppError::from))
    .transpose()
}
fn rate(book: &FxBook, currency: &str) -> (Option<f64>, String, String) {
    if currency.eq_ignore_ascii_case(&book.base_currency) {
        return (None, String::new(), String::new());
    }
    if let Some(m) = book.manual.get(currency) {
        return (Some(m.rate), "central_manual".into(), m.observed_on.clone());
    }
    if let Some(q) = book.quotes.iter().find(|q| q.from_currency == currency) {
        return (Some(q.rate), "central_daily".into(), q.observed_on.clone());
    }
    (None, String::new(), String::new())
}
pub(super) fn holding_input(
    conn: &Connection,
    input: &HoldingInput,
    base: &str,
) -> AppResult<HoldingInput> {
    let mut result = input.clone();
    if let Some(book) = book_on(conn, base)? {
        (
            result.fx_rate_to_base,
            result.fx_rate_source,
            result.fx_rate_observed_on,
        ) = rate(&book, &input.currency.trim().to_ascii_uppercase());
    }
    Ok(result)
}
pub(super) fn apply_holdings(conn: &Connection) -> AppResult<()> {
    let snapshot = Database::snapshot_on(conn)?;
    let Some(book) = book_on(conn, &snapshot.profile.base_currency)? else {
        return Ok(());
    };
    for h in snapshot.holdings {
        let (value, source, observed) = rate(&book, &h.currency);
        if h.fx_rate_to_base != value
            || h.fx_rate_source != source
            || h.fx_rate_observed_on != observed
        {
            conn.execute("UPDATE holdings SET fx_rate_to_base=?2,fx_rate_source=?3,fx_rate_observed_on=?4,updated_at=?5 WHERE id=?1",
                params![h.id, value, source, observed, Utc::now().to_rfc3339()])?;
        }
    }
    Ok(())
}
impl Database {
    pub fn fx_book(&self, base: &str) -> AppResult<FxBook> {
        Ok(book_on(&*self.conn()?, base)?.unwrap_or_else(|| FxBook {
            base_currency: base.into(),
            ..Default::default()
        }))
    }
    pub fn save_fx_book(&self, book: &FxBook) -> AppResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        // Refreshing rates must not choose a new account's bookkeeping timezone.
        // The first client daily-assets/ensure request still owns that choice.
        let initialized: bool =
            tx.query_row("SELECT EXISTS(SELECT 1 FROM daily_settings)", [], |r| {
                r.get(0)
            })?;
        let before = if initialized {
            Some(daily::prepare(&tx)?)
        } else {
            None
        };
        tx.execute("INSERT INTO settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key(&book.base_currency), serde_json::to_string(book)?])?;
        apply_holdings(&tx)?;
        if let Some(before) = before {
            daily::finish(&tx, &before, None)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn set_fx_override(&self, input: &FxOverrideInput) -> AppResult<FxBook> {
        let base = self.snapshot()?.profile.base_currency;
        let currency = input.currency.trim().to_ascii_uppercase();
        if currency == base
            || !crate::market_data::SUPPORTED_CURRENCIES.contains(&currency.as_str())
        {
            return Err(AppError::Validation("请选择需要换算的外币".into()));
        }
        let mut book = self.fx_book(&base)?;
        if let Some(value) = input.rate {
            if !value.is_finite() || value <= 0.0 || value > 1e9 {
                return Err(AppError::Validation(
                    "汇率必须是有效正数，且不超过 10 亿".into(),
                ));
            }
            book.manual.insert(
                currency,
                ManualRate {
                    rate: value,
                    observed_on: crate::fx::today(),
                },
            );
        } else {
            book.manual.remove(&currency);
        }
        self.save_fx_book(&book)?;
        Ok(book)
    }
    pub fn centralized_event(
        &self,
        input: &PortfolioEventInput,
        base: &str,
    ) -> AppResult<PortfolioEventInput> {
        let mut input = input.clone();
        let from = input.currency.trim().to_ascii_uppercase();
        // Legacy direct database usage remains supported until the account has a central book.
        if book_on(&*self.conn()?, base)?.is_none() {
            return Ok(input);
        }
        if from == base {
            input.fx_rate_to_base = None;
            input.fx_rate_source.clear();
            input.fx_rate_observed_on.clear();
            return Ok(input);
        }
        // Re-importing an unchanged source transaction uses its frozen FX evidence,
        // even if the daily book has since changed. Changed user facts still conflict.
        let fingerprint = portfolio_event_fingerprint(&input);
        if !fingerprint.is_empty() {
            let conn = self.conn()?;
            let existing = conn.query_row("SELECT amount,currency,asset_name,occurred_on,note,event_type,fx_rate_to_base,fx_rate_source,fx_rate_observed_on FROM portfolio_events WHERE fingerprint=?1 AND reversal_of_event_id IS NULL", [&fingerprint], |r| {
                Ok((r.get::<_,f64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<f64>>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?))
            }).optional()?;
            if let Some((
                amount,
                currency,
                asset_name,
                day,
                note,
                event_type,
                value,
                source,
                observed,
            )) = existing
            {
                if amount == input.amount
                    && currency == from
                    && asset_name == input.asset_name.trim()
                    && day == input.occurred_on.trim()
                    && note == input.note.trim()
                    && serde_json::to_value(&input.event_type)?.as_str()
                        == Some(event_type.as_str())
                {
                    input.fx_rate_to_base = value;
                    input.fx_rate_source = source;
                    input.fx_rate_observed_on = observed;
                    return Ok(input);
                }
            }
        }
        let stored = self.setting(&format!("fx.quote.{from}.{base}.{}", input.occurred_on))?;
        let quote: Option<crate::models::FxRateQuote> =
            stored.map(|s| serde_json::from_str(&s)).transpose()?;
        if let Some(quote) = quote {
            input.fx_rate_to_base = Some(quote.rate);
            input.fx_rate_source = quote.provider_code;
            input.fx_rate_observed_on = quote.observed_on;
        } else {
            input.fx_rate_to_base = None;
            input.fx_rate_source.clear();
            input.fx_rate_observed_on.clear();
        }
        Ok(input)
    }
}

// Only currency data may enter the exported settings subset. Model keys and other
// account configuration remain outside data exports and cannot be overwritten.
pub(super) fn validate_sync(dataset: &SyncDataset) -> AppResult<()> {
    let Some(table) = dataset.tables.iter().find(|t| t.name == "settings") else {
        return Ok(());
    };
    let invalid = || AppError::Validation("统一汇率设置无效".into());
    let supported = |c: &str| crate::market_data::SUPPORTED_CURRENCIES.contains(&c);
    let valid_rate = |v: f64| v.is_finite() && v > 0.0 && v <= 1e9;
    let valid_day = |s: &str| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() && s <= crate::fx::today().as_str()
    };
    let valid_quote = |q: &crate::models::FxRateQuote| {
        supported(&q.from_currency)
            && supported(&q.to_currency)
            && valid_rate(q.rate)
            && valid_day(&q.requested_on)
            && valid_day(&q.observed_on)
            && q.observed_on <= q.requested_on
    };
    let mut seen = HashSet::new();
    for row in &table.rows {
        let [SyncValue::Text(key), SyncValue::Text(value)] = row.as_slice() else {
            return Err(invalid());
        };
        if !seen.insert(key) {
            return Err(invalid());
        }
        if let Some(base) = key.strip_prefix("fx.book.") {
            let book: FxBook = serde_json::from_str(value).map_err(|_| invalid())?;
            if !supported(base)
                || book.base_currency != base
                || book.checked_at < 0
                || book.checked_at > Utc::now().timestamp() + 60
                || (!book.requested_on.is_empty() && !valid_day(&book.requested_on))
                || book.quotes.len() > 6
                || book.manual.len() > 5
                || book.error.len() > 16000
                || book
                    .quotes
                    .iter()
                    .any(|q| q.to_currency != base || !valid_quote(q))
                || book.manual.iter().any(|(c, m)| {
                    !supported(c) || c == base || !valid_rate(m.rate) || !valid_day(&m.observed_on)
                })
            {
                return Err(invalid());
            }
        } else if key.starts_with("fx.quote.") {
            let q: crate::models::FxRateQuote =
                serde_json::from_str(value).map_err(|_| invalid())?;
            if !valid_quote(&q)
                || key
                    != &format!(
                        "fx.quote.{}.{}.{}",
                        q.from_currency, q.to_currency, q.requested_on
                    )
            {
                return Err(invalid());
            }
        } else {
            return Err(invalid());
        }
    }
    Ok(())
}
