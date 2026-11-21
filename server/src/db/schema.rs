use crate::{error::AppResult, storage::Connection};

// Bump whenever hosted tables or indexes change so existing tenants receive new SQL objects.
pub(super) const POSTGRES_SCHEMA_VERSION: i64 = 23;

pub(super) fn initialize(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS watch_entries (
           symbol TEXT PRIMARY KEY, payload TEXT NOT NULL, checked_at TEXT, checked_for TEXT,
           refresh_failed INTEGER NOT NULL DEFAULT 0 CHECK (refresh_failed IN (0,1))
         );
         CREATE TABLE IF NOT EXISTS watch_observations (
           symbol TEXT NOT NULL REFERENCES watch_entries(symbol) ON DELETE CASCADE DEFERRABLE INITIALLY IMMEDIATE,
           day TEXT NOT NULL, payload TEXT NOT NULL, alert INTEGER NOT NULL CHECK (alert IN (0,1)), read_at TEXT,
           PRIMARY KEY(symbol,day)
         );
         CREATE TABLE IF NOT EXISTS watch_dividends (
           symbol TEXT PRIMARY KEY REFERENCES watch_entries(symbol) ON DELETE CASCADE DEFERRABLE INITIALLY IMMEDIATE,
           payload TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS daily_settings (
           id TEXT PRIMARY KEY, timezone TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS daily_entries (
           id TEXT PRIMARY KEY, day TEXT NOT NULL, entity_id TEXT NOT NULL,
           payload TEXT NOT NULL, UNIQUE(day, entity_id)
         );
         CREATE INDEX IF NOT EXISTS daily_entries_day ON daily_entries(day);
         CREATE TABLE IF NOT EXISTS daily_totals (
           day TEXT PRIMARY KEY, payload TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS profile (
           id INTEGER PRIMARY KEY CHECK (id = 1),
           payload TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS holdings (
           id TEXT PRIMARY KEY,
           symbol TEXT NOT NULL,
           name TEXT NOT NULL,
           asset_class TEXT NOT NULL,
           market_value DOUBLE PRECISION NOT NULL,
           cost_basis DOUBLE PRECISION NOT NULL,
           target_pct DOUBLE PRECISION NOT NULL DEFAULT 0,
           currency TEXT NOT NULL,
           fx_rate_to_base DOUBLE PRECISION,
           valuation_date TEXT NOT NULL DEFAULT '',
           fx_rate_source TEXT NOT NULL DEFAULT '',
           fx_rate_observed_on TEXT NOT NULL DEFAULT '',
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL DEFAULT '',
           cost_price DOUBLE PRECISION,
           quantity DOUBLE PRECISION,
           current_price DOUBLE PRECISION
         );
         CREATE TABLE IF NOT EXISTS analyses (
           id TEXT PRIMARY KEY,
           question TEXT NOT NULL,
           answer TEXT NOT NULL,
           audit TEXT,
           trace TEXT,
           created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS research_evidence (
           id TEXT PRIMARY KEY,
           asset_name TEXT NOT NULL,
           title TEXT NOT NULL,
           publisher TEXT NOT NULL,
           source_url TEXT NOT NULL,
           source_tier TEXT NOT NULL,
           evidence_type TEXT NOT NULL,
           stance TEXT NOT NULL,
           as_of_date TEXT NOT NULL,
           claim TEXT NOT NULL,
           notes TEXT NOT NULL,
           active INTEGER NOT NULL,
           captured_at TEXT NOT NULL,
           category TEXT NOT NULL DEFAULT ''
         );
         CREATE TABLE IF NOT EXISTS research_attachments (
           id TEXT PRIMARY KEY,
           evidence_id TEXT NOT NULL REFERENCES research_evidence(id) ON DELETE CASCADE DEFERRABLE INITIALLY IMMEDIATE,
           name TEXT NOT NULL,
           byte_size INTEGER NOT NULL,
           data_base64 TEXT NOT NULL,
           extracted_text TEXT NOT NULL,
           extraction_note TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS research_attachments_evidence ON research_attachments(evidence_id);
         CREATE TABLE IF NOT EXISTS holding_valuations (
           holding_id TEXT PRIMARY KEY REFERENCES holdings(id) ON DELETE CASCADE DEFERRABLE INITIALLY IMMEDIATE,
           symbol TEXT NOT NULL,
           quantity DOUBLE PRECISION NOT NULL,
           unit_price DOUBLE PRECISION NOT NULL,
           market_value DOUBLE PRECISION NOT NULL,
           currency TEXT NOT NULL,
           requested_on TEXT NOT NULL,
           observed_on TEXT NOT NULL,
           staleness_days INTEGER NOT NULL,
           provider_code TEXT NOT NULL,
           provider_name TEXT NOT NULL,
           exchange_name TEXT NOT NULL,
           mic_code TEXT NOT NULL,
           instrument_type TEXT NOT NULL,
           price_basis TEXT NOT NULL,
           source_url TEXT NOT NULL,
           methodology_url TEXT NOT NULL,
           disclaimer TEXT NOT NULL,
           captured_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS settings (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS analysis_memories (
           id TEXT PRIMARY KEY,
           kind TEXT NOT NULL CHECK (kind IN ('preference','viewpoint','context')),
           origin TEXT NOT NULL CHECK (origin IN ('userStated','modelInferred')),
           statement TEXT NOT NULL,
           rationale TEXT NOT NULL,
           source_analysis_ids TEXT NOT NULL,
           quote TEXT NOT NULL,
           active INTEGER NOT NULL CHECK (active IN (0,1)),
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS daily_review_runs (
           id TEXT PRIMARY KEY,
           review_day TEXT NOT NULL,
           trigger TEXT NOT NULL,
           status TEXT NOT NULL,
           payload TEXT NOT NULL,
           created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_daily_review_runs_day ON daily_review_runs(review_day DESC, created_at DESC);
         CREATE TABLE IF NOT EXISTS daily_review_items (
           id TEXT PRIMARY KEY,
           run_id TEXT NOT NULL REFERENCES daily_review_runs(id) ON DELETE CASCADE DEFERRABLE INITIALLY IMMEDIATE,
           review_day TEXT NOT NULL,
           kind TEXT NOT NULL,
           symbol TEXT NOT NULL DEFAULT '',
           importance TEXT NOT NULL,
           payload TEXT NOT NULL,
           read_at TEXT,
           created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_daily_review_items_day ON daily_review_items(review_day DESC, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_daily_review_items_unread ON daily_review_items(review_day DESC) WHERE read_at IS NULL;
         CREATE TABLE IF NOT EXISTS holding_trades (
           id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES holdings(id) DEFERRABLE INITIALLY IMMEDIATE,
           payload TEXT NOT NULL, created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_holding_trades_holding ON holding_trades(holding_id);
         CREATE TABLE IF NOT EXISTS holding_options (
           holding_id TEXT PRIMARY KEY REFERENCES holdings(id) ON DELETE CASCADE DEFERRABLE INITIALLY IMMEDIATE,
           underlying_id TEXT NOT NULL REFERENCES holdings(id) DEFERRABLE INITIALLY IMMEDIATE,
           payload TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS holding_options_underlying ON holding_options(underlying_id);
         CREATE INDEX IF NOT EXISTS idx_analyses_created_at ON analyses(created_at DESC, id DESC);
         CREATE INDEX IF NOT EXISTS idx_research_evidence_list ON research_evidence(active DESC, captured_at DESC, id ASC);
         CREATE INDEX IF NOT EXISTS idx_watch_observations_unread ON watch_observations(symbol) WHERE alert = 1 AND read_at IS NULL;
         CREATE INDEX IF NOT EXISTS idx_holdings_created_at ON holdings(created_at);
         DROP TABLE IF EXISTS trade_settlements;",
    )?;
    super::financial::initialize(connection)?;
    Ok(())
}
