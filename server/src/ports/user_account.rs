//! User-owned account capabilities. No AI state, credentials, SQL or arbitrary settings.
use super::account::AccountSession;
use crate::{
    models::{
        accounting::DividendIncome,
        daily::{DailyHistory, EnsureInput, HistoryQuery},
        fx::{FxBook, FxOverrideInput},
        market::{
            dividends::DividendHistory,
            quotes::{FxRateQuote, SecurityPriceQuote},
        },
        portfolio::{HoldingInput, Snapshot},
        watch::{Observation, WatchBoard, WatchEntry},
    },
    AppResult,
};
use chrono::NaiveDate;
use std::collections::BTreeMap;

pub(crate) trait PortfolioState: AccountSession {
    fn snapshot(&self) -> AppResult<Snapshot>;
    fn base_currency(&self) -> AppResult<String> {
        Ok(self.snapshot()?.profile.base_currency)
    }
    fn revision(&self) -> AppResult<String>;
    fn require_revision(&self, expected: &str) -> AppResult<()>;
}

pub(crate) trait FxAccount: PortfolioState {
    fn fx_book(&self, base: &str) -> AppResult<FxBook>;
    fn cached_fx_quote(
        &self,
        from: &str,
        to: &str,
        on_date: &str,
    ) -> AppResult<Option<FxRateQuote>>;
    fn save_fx_book(&self, book: &FxBook) -> AppResult<()>;
}

pub(crate) trait ValuationAccount: PortfolioState {
    fn register_corporate_actions(&self, id: &str, quote: &SecurityPriceQuote) -> AppResult<bool>;
    fn apply_verified_holding_valuation(
        &self,
        id: &str,
        quantity: f64,
        quote: &SecurityPriceQuote,
    ) -> AppResult<Snapshot>;
}

pub(crate) trait PortfolioAccount: FxAccount + ValuationAccount {
    fn set_fx_override(&self, input: &FxOverrideInput) -> AppResult<FxBook>;
    fn add_holding(&self, input: &HoldingInput) -> AppResult<Snapshot>;
    fn update_holding(&self, id: &str, input: &HoldingInput) -> AppResult<Snapshot>;
    fn ensure_daily(&self, input: &EnsureInput) -> AppResult<DailyHistory>;
    fn daily_history(&self, query: &HistoryQuery) -> AppResult<DailyHistory>;
    fn trailing_dividends(
        &self,
        histories: &BTreeMap<String, DividendHistory>,
        today: NaiveDate,
    ) -> AppResult<DividendIncome>;
}

pub(crate) trait WatchAccount: AccountSession {
    fn watch_board(&self, today: NaiveDate) -> AppResult<WatchBoard>;
    fn watch_dividends(&self, symbol: &str) -> AppResult<Option<DividendHistory>>;
    fn watch_dividends_all(&self) -> AppResult<BTreeMap<String, DividendHistory>> {
        Ok(BTreeMap::new())
    }
    fn save_watch_observation(
        &self,
        expected: &WatchEntry,
        result: Option<&Observation>,
        checked_at: &str,
        checked_for: NaiveDate,
        dividends: Option<&DividendHistory>,
    ) -> AppResult<()>;
}
