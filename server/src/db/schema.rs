use rusqlite::Connection;

use crate::error::AppResult;

// Keep legacy migration order intact: opening an old database remains idempotent.
pub(super) fn initialize(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS profile (
           id INTEGER PRIMARY KEY CHECK (id = 1),
           payload TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS goals (
           id TEXT PRIMARY KEY,
           name TEXT NOT NULL,
           target_amount REAL NOT NULL,
           current_amount REAL NOT NULL DEFAULT 0,
           monthly_contribution REAL NOT NULL DEFAULT 0,
           target_date TEXT NOT NULL,
           priority TEXT NOT NULL,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS holdings (
           id TEXT PRIMARY KEY,
           symbol TEXT NOT NULL,
           name TEXT NOT NULL,
           asset_class TEXT NOT NULL,
           market_value REAL NOT NULL,
           cost_basis REAL NOT NULL,
           target_pct REAL NOT NULL DEFAULT 0,
           currency TEXT NOT NULL,
           fx_rate_to_base REAL,
           valuation_date TEXT NOT NULL DEFAULT '',
           fx_rate_source TEXT NOT NULL DEFAULT '',
           fx_rate_observed_on TEXT NOT NULL DEFAULT '',
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS decisions (
           id TEXT PRIMARY KEY,
           asset_name TEXT NOT NULL,
           payload TEXT NOT NULL,
           review_date TEXT NOT NULL,
           created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS decision_reviews (
           decision_id TEXT PRIMARY KEY REFERENCES decisions(id) ON DELETE CASCADE,
           outcome_summary TEXT NOT NULL,
           actual_return_pct REAL,
           thesis_status TEXT NOT NULL,
           process_rating INTEGER NOT NULL,
           lessons TEXT NOT NULL,
           reviewed_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS analyses (
           id TEXT PRIMARY KEY,
           question TEXT NOT NULL,
           answer TEXT NOT NULL,
           audit TEXT,
           trace TEXT,
           created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS system_reviews (
           id TEXT PRIMARY KEY,
           payload TEXT NOT NULL,
           snapshot TEXT NOT NULL,
           next_review_date TEXT NOT NULL,
           created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS investment_rules (
           id TEXT PRIMARY KEY,
           category TEXT NOT NULL,
           statement TEXT NOT NULL,
           trigger TEXT NOT NULL,
           rationale TEXT NOT NULL,
           active INTEGER NOT NULL,
           source_review_id TEXT,
           revision INTEGER NOT NULL,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL,
           FOREIGN KEY(source_review_id) REFERENCES system_reviews(id)
         );
         CREATE TABLE IF NOT EXISTS investment_rule_revisions (
           rule_id TEXT NOT NULL,
           revision INTEGER NOT NULL,
           payload TEXT NOT NULL,
           changed_at TEXT NOT NULL,
           PRIMARY KEY(rule_id, revision),
           FOREIGN KEY(rule_id) REFERENCES investment_rules(id) ON DELETE CASCADE
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
           captured_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS portfolio_checkins (
           id TEXT PRIMARY KEY,
           payload TEXT NOT NULL,
           created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS portfolio_events (
           id TEXT PRIMARY KEY,
           event_type TEXT NOT NULL,
           source TEXT NOT NULL DEFAULT 'manual',
           external_id TEXT NOT NULL DEFAULT '',
           fingerprint TEXT NOT NULL DEFAULT '',
           asset_name TEXT NOT NULL,
           amount REAL NOT NULL,
           currency TEXT NOT NULL,
           fx_rate_to_base REAL,
           fx_rate_source TEXT NOT NULL DEFAULT '',
           fx_rate_observed_on TEXT NOT NULL DEFAULT '',
           base_currency TEXT NOT NULL,
           base_amount REAL NOT NULL,
           occurred_on TEXT NOT NULL,
           note TEXT NOT NULL,
           created_at TEXT NOT NULL,
           reversal_of_event_id TEXT REFERENCES portfolio_events(id)
         );
         CREATE TABLE IF NOT EXISTS memory_preferences (
           memory_id TEXT PRIMARY KEY,
           preference TEXT NOT NULL,
           note TEXT NOT NULL DEFAULT '',
           updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS holding_valuations (
           holding_id TEXT PRIMARY KEY REFERENCES holdings(id) ON DELETE CASCADE,
           symbol TEXT NOT NULL,
           quantity REAL NOT NULL,
           unit_price REAL NOT NULL,
           market_value REAL NOT NULL,
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
         );",
    )?;
    ensure_column(
        connection,
        "goals",
        "current_amount",
        "REAL NOT NULL DEFAULT 0",
    )?;
    ensure_column(connection, "analyses", "audit", "TEXT")?;
    ensure_column(connection, "analyses", "trace", "TEXT")?;
    ensure_column(
        connection,
        "goals",
        "updated_at",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "goals",
        "monthly_contribution",
        "REAL NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "holdings",
        "target_pct",
        "REAL NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "holdings",
        "updated_at",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "portfolio_events",
        "source",
        "TEXT NOT NULL DEFAULT 'manual'",
    )?;
    ensure_column(
        connection,
        "portfolio_events",
        "external_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "portfolio_events",
        "fingerprint",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_portfolio_events_occurred_on
           ON portfolio_events(occurred_on, created_at);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_portfolio_events_fingerprint
           ON portfolio_events(fingerprint) WHERE fingerprint <> '';",
    )?;
    ensure_column(connection, "holdings", "fx_rate_to_base", "REAL")?;
    ensure_column(
        connection,
        "holdings",
        "valuation_date",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "holdings",
        "fx_rate_source",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "holdings",
        "fx_rate_observed_on",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "portfolio_events",
        "fx_rate_source",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "portfolio_events",
        "fx_rate_observed_on",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "portfolio_events",
        "reversal_of_event_id",
        "TEXT REFERENCES portfolio_events(id)",
    )?;
    connection.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_portfolio_events_single_reversal
           ON portfolio_events(reversal_of_event_id) WHERE reversal_of_event_id IS NOT NULL;",
    )?;
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> AppResult<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|value| value == column) {
        connection.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}
