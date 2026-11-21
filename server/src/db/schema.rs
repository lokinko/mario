use crate::storage::Connection;

use crate::error::AppResult;
use crate::storage::OptionalRow;

// Bump whenever hosted tables/columns or one-time data migrations change. This is independent of the
// export format version: an existing tenant must also receive new SQL objects.
pub(super) const POSTGRES_SCHEMA_VERSION: i64 = 21;

// Keep migration order intact: re-running on an existing tenant schema is idempotent.
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
           FOREIGN KEY(source_review_id) REFERENCES system_reviews(id) DEFERRABLE INITIALLY IMMEDIATE
         );
         CREATE TABLE IF NOT EXISTS investment_rule_revisions (
           rule_id TEXT NOT NULL,
           revision INTEGER NOT NULL,
           payload TEXT NOT NULL,
           changed_at TEXT NOT NULL,
           PRIMARY KEY(rule_id, revision),
           FOREIGN KEY(rule_id) REFERENCES investment_rules(id) ON DELETE CASCADE DEFERRABLE INITIALLY IMMEDIATE
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
         CREATE INDEX IF NOT EXISTS idx_daily_review_items_unread ON daily_review_items(review_day DESC) WHERE read_at IS NULL;",
    )?;
    connection.execute_batch(
        "ALTER TABLE analyses ADD COLUMN IF NOT EXISTS audit TEXT;
         ALTER TABLE analyses ADD COLUMN IF NOT EXISTS trace TEXT;
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS target_pct DOUBLE PRECISION NOT NULL DEFAULT 0;
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS updated_at TEXT NOT NULL DEFAULT '';
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS fx_rate_to_base DOUBLE PRECISION;
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS cost_price DOUBLE PRECISION;
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS quantity DOUBLE PRECISION;
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS current_price DOUBLE PRECISION;
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS valuation_date TEXT NOT NULL DEFAULT '';
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS fx_rate_source TEXT NOT NULL DEFAULT '';
         ALTER TABLE holdings ADD COLUMN IF NOT EXISTS fx_rate_observed_on TEXT NOT NULL DEFAULT '';
         ALTER TABLE research_evidence ADD COLUMN IF NOT EXISTS category TEXT NOT NULL DEFAULT '';",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS holding_trades (
           id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES holdings(id) DEFERRABLE INITIALLY IMMEDIATE,
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
    // Migrate legacy per-currency cash_state primary key to per-account holding_id.
    connection.execute_batch(
        "DO $$
         DECLARE pk_name text;
         DECLARE pk_cols text;
         BEGIN
           SELECT c.conname, string_agg(a.attname, ',' ORDER BY u.ord)
           INTO pk_name, pk_cols
           FROM pg_constraint c
           CROSS JOIN LATERAL unnest(c.conkey) WITH ORDINALITY AS u(attnum, ord)
           JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = u.attnum
           WHERE c.conrelid = to_regclass('cash_state') AND c.contype = 'p'
           GROUP BY c.conname;
           IF pk_name IS NOT NULL AND pk_cols <> 'holding_id' THEN
             EXECUTE format('ALTER TABLE cash_state DROP CONSTRAINT %I', pk_name);
             ALTER TABLE cash_state ADD PRIMARY KEY (holding_id);
           END IF;
         END $$;",
    )?;
    connection.execute_batch("CREATE TABLE IF NOT EXISTS holding_options (
        holding_id TEXT PRIMARY KEY REFERENCES holdings(id) ON DELETE CASCADE DEFERRABLE INITIALLY IMMEDIATE,
        underlying_id TEXT NOT NULL REFERENCES holdings(id) DEFERRABLE INITIALLY IMMEDIATE,
        payload TEXT NOT NULL
    ); CREATE INDEX IF NOT EXISTS holding_options_underlying ON holding_options(underlying_id);")?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_analyses_created_at ON analyses(created_at DESC, id DESC);
         CREATE INDEX IF NOT EXISTS idx_research_evidence_list ON research_evidence(active DESC, captured_at DESC, id ASC);
         CREATE INDEX IF NOT EXISTS idx_watch_observations_unread ON watch_observations(symbol) WHERE alert = 1 AND read_at IS NULL;
         CREATE INDEX IF NOT EXISTS idx_holdings_created_at ON holdings(created_at);",
    )?;
    super::watch::migrate_rules(connection)?;
    super::watch_grid_migration::migrate(connection)?;
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
            let canonical = canonical_analysis_preset(&mut value);
            if canonical {
                connection.execute(
                    "UPDATE settings SET value=$1 WHERE key='analysis.settings'",
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
                &format!("UPDATE analyses SET {column}=$1 WHERE id=$2"),
                crate::storage::params![serde_json::to_string(&value)?, id],
            )?;
        }
    }
    Ok(())
}

/// Remove retired preset options and move every stored workflow to the single current one,
/// so strict request and preset parsing never meets old keys. Returns whether it changed.
fn canonical_analysis_preset(value: &mut serde_json::Value) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    for key in crate::models::analysis::RETIRED_ANALYSIS_OPTIONS {
        changed |= object.remove(key).is_some();
    }
    if let Some(selection) = object
        .get_mut("contextSelection")
        .and_then(serde_json::Value::as_object_mut)
    {
        for key in crate::models::analysis::RETIRED_CONTEXT_OPTIONS {
            changed |= selection.remove(key).is_some();
        }
    }
    if object.get("workflow").and_then(serde_json::Value::as_str)
        != Some(crate::models::analysis::ANALYSIS_WORKFLOW)
    {
        object.insert(
            "workflow".into(),
            crate::models::analysis::ANALYSIS_WORKFLOW.into(),
        );
        changed = true;
    }
    changed
}
