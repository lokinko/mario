use crate::storage::Connection;

use super::OptionalRow;
use crate::error::AppResult;

// Bump whenever hosted tables/columns or one-time data migrations change. This is independent of the
// export format version: an existing tenant must also receive new SQL objects.
pub(super) const POSTGRES_SCHEMA_VERSION: i64 = 10;

// Keep legacy migration order intact: opening an old database remains idempotent.
pub(super) fn initialize(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
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
         CREATE TABLE IF NOT EXISTS research_attachments (
           id TEXT PRIMARY KEY,
           evidence_id TEXT NOT NULL REFERENCES research_evidence(id) ON DELETE CASCADE,
           name TEXT NOT NULL,
           byte_size INTEGER NOT NULL,
           data_base64 TEXT NOT NULL,
           extracted_text TEXT NOT NULL,
           extraction_note TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS research_attachments_evidence ON research_attachments(evidence_id);
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
    ensure_column(connection, "analyses", "audit", "TEXT")?;
    ensure_column(connection, "analyses", "trace", "TEXT")?;
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
    ensure_column(connection, "holdings", "fx_rate_to_base", "REAL")?;
    ensure_column(connection, "holdings", "cost_price", "REAL")?;
    ensure_column(connection, "holdings", "quantity", "REAL")?;
    ensure_column(connection, "holdings", "current_price", "REAL")?;
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
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS holding_trades (
           id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES holdings(id),
           payload TEXT NOT NULL, created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_holding_trades_holding ON holding_trades(holding_id);",
    )?;
    connection
        .execute_batch("DROP TABLE IF EXISTS portfolio_events; DROP TABLE IF EXISTS goals; DROP TABLE IF EXISTS memory_preferences;")?;
    // Canonicalize legacy settings once at the storage boundary.
    connection.execute_batch(
        "UPDATE settings SET value='openai-responses'
           WHERE key='model.provider' AND value='openai-compatible';
         UPDATE settings SET value=replace(value,'openai-compatible ','openai-responses ')
           WHERE key='model.key_binding' AND value LIKE 'openai-compatible %';
         INSERT INTO settings(key,value)
           SELECT 'model.key_binding',
             coalesce((SELECT value FROM settings WHERE key='model.provider'),'openai-responses') || ' ' ||
             rtrim(coalesce((SELECT value FROM settings WHERE key='model.base_url'),'https://api.openai.com/v1'),'/')
           WHERE EXISTS(SELECT 1 FROM settings WHERE key='secret.model')
             AND coalesce((SELECT value FROM settings WHERE key='model.provider'),'openai-responses')
                 NOT IN ('codex','codex-bridge')
           ON CONFLICT(key) DO NOTHING;",
    )?;
    remove_retired_analysis_metadata(connection)?;
    connection.execute("DELETE FROM settings WHERE key LIKE 'trading.reset.%'", [])?;
    super::financial::initialize(connection)?;
    Ok(())
}

// Strip retired fields without rewriting answers or their frozen source catalog.
// Also run after imports so legacy backups cannot restore removed metadata.
pub(super) fn remove_retired_analysis_metadata(connection: &Connection) -> AppResult<()> {
    let preset: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key='analysis.settings'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(preset) = preset {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&preset) {
            if value
                .as_object_mut()
                .is_some_and(|object| object.remove("useMemory").is_some())
            {
                connection.execute(
                    "UPDATE settings SET value=?1 WHERE key='analysis.settings'",
                    [serde_json::to_string(&value)?],
                )?;
            }
        }
    }
    for (column, fields) in [
        (
            "audit",
            &[
                "memoryItemsUsed",
                "reviewedMemoryItemsUsed",
                "conflictingMemoryItemsUsed",
            ][..],
        ),
        ("trace", &["memoryItems"][..]),
    ] {
        let filter = fields
            .iter()
            .map(|field| format!("{column} LIKE '%\"{field}\"%'"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut statement =
            connection.prepare(&format!("SELECT id,{column} FROM analyses WHERE {filter}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for (id, raw) in rows {
            let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            let Some(object) = value.as_object_mut() else {
                continue;
            };
            for field in fields {
                object.remove(*field);
            }
            if column == "audit" {
                if let Some(groups) = object
                    .get_mut("contextGroups")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    groups.retain(|group| group.as_str() != Some("相关历史记忆"));
                }
            }
            connection.execute(
                &format!("UPDATE analyses SET {column}=?1 WHERE id=?2"),
                super::params![serde_json::to_string(&value)?, id],
            )?;
        }
    }
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> AppResult<()> {
    if connection.is_postgres() {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN IF NOT EXISTS {column} {definition}"
        ))?;
        return Ok(());
    }
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
