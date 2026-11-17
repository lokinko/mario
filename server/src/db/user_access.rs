//! User capability adapters preserve the existing account transactions and revision checks.
use super::Database;
use crate::{
    models::{
        accounting::DividendIncome,
        daily::{DailyHistory, EnsureInput, HistoryQuery},
        fx::{FxBook, FxOverrideInput},
        market::quotes::{FxRateQuote, SecurityPriceQuote},
        portfolio::{HoldingInput, Snapshot},
        watch::{DividendHistory, Observation, WatchBoard, WatchEntry},
    },
    ports::user_account::{
        FxAccount, PortfolioAccount, PortfolioState, ValuationAccount, WatchAccount,
    },
    AppResult,
};
use chrono::NaiveDate;
use std::collections::BTreeMap;

impl PortfolioState for Database {
    fn snapshot(&self) -> AppResult<Snapshot> {
        Database::snapshot(self)
    }
    fn revision(&self) -> AppResult<String> {
        Database::revision(self)
    }
    fn require_revision(&self, expected: &str) -> AppResult<()> {
        Database::require_revision(self, expected)
    }
}
impl FxAccount for Database {
    fn fx_book(&self, base: &str) -> AppResult<FxBook> {
        Database::fx_book(self, base)
    }
    fn cached_fx_quote(
        &self,
        from: &str,
        to: &str,
        on_date: &str,
    ) -> AppResult<Option<FxRateQuote>> {
        self.setting(&format!("fx.quote.{from}.{to}.{on_date}"))?
            .map(|stored| Ok(serde_json::from_str(&stored)?))
            .transpose()
    }
    fn save_fx_book(&self, book: &FxBook) -> AppResult<()> {
        Database::save_fx_book(self, book)
    }
}
impl ValuationAccount for Database {
    fn register_corporate_actions(&self, id: &str, quote: &SecurityPriceQuote) -> AppResult<bool> {
        Database::register_corporate_actions(self, id, quote)
    }
    fn apply_verified_holding_valuation(
        &self,
        id: &str,
        quantity: f64,
        quote: &SecurityPriceQuote,
    ) -> AppResult<Snapshot> {
        Database::apply_verified_holding_valuation(self, id, quantity, quote)
    }
}
impl PortfolioAccount for Database {
    fn set_fx_override(&self, input: &FxOverrideInput) -> AppResult<FxBook> {
        Database::set_fx_override(self, input)
    }
    fn add_holding(&self, input: &HoldingInput) -> AppResult<Snapshot> {
        Database::add_holding(self, input)
    }
    fn update_holding(&self, id: &str, input: &HoldingInput) -> AppResult<Snapshot> {
        Database::update_holding(self, id, input)
    }
    fn ensure_daily(&self, input: &EnsureInput) -> AppResult<DailyHistory> {
        Database::ensure_daily(self, input)
    }
    fn daily_history(&self, query: &HistoryQuery) -> AppResult<DailyHistory> {
        Database::daily_history(self, query)
    }
    fn trailing_dividends(
        &self,
        histories: &BTreeMap<String, DividendHistory>,
        today: NaiveDate,
    ) -> AppResult<DividendIncome> {
        Database::trailing_dividends(self, histories, today)
    }
}
impl WatchAccount for Database {
    fn watch_board(&self, today: NaiveDate) -> AppResult<WatchBoard> {
        Database::watch_board(self, today)
    }
    fn watch_dividends(&self, symbol: &str) -> AppResult<Option<DividendHistory>> {
        Database::watch_dividends(self, symbol)
    }
    fn save_watch_observation(
        &self,
        expected: &WatchEntry,
        result: Option<&Observation>,
        checked_at: &str,
        checked_for: NaiveDate,
        dividends: Option<&DividendHistory>,
    ) -> AppResult<()> {
        Database::save_watch_observation(self, expected, result, checked_at, checked_for, dividends)
    }
}
