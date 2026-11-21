use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SyncValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTable {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<SyncValue>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDataset {
    pub schema_version: u32,
    pub exported_at: String,
    pub tables: Vec<SyncTable>,
}

#[derive(Clone, Copy)]
pub(super) struct SyncTableSpec {
    pub(super) name: &'static str,
    pub(super) columns: &'static [&'static str],
    pub(super) order_by: &'static str,
}

const CURRENT_HOLDING_COLUMNS: &[&str] = &[
    "id",
    "symbol",
    "name",
    "asset_class",
    "market_value",
    "cost_basis",
    "target_pct",
    "currency",
    "fx_rate_to_base",
    "valuation_date",
    "fx_rate_source",
    "fx_rate_observed_on",
    "created_at",
    "updated_at",
    "cost_price",
    "quantity",
    "current_price",
];

pub(super) const SYNC_TABLES: &[SyncTableSpec] = &[
    SyncTableSpec {
        name: "profile",
        columns: &["id", "payload", "updated_at"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "holdings",
        columns: CURRENT_HOLDING_COLUMNS,
        order_by: "id",
    },
    SyncTableSpec {
        name: "analyses",
        columns: &["id", "question", "answer", "audit", "trace", "created_at"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "research_evidence",
        columns: &[
            "id",
            "asset_name",
            "title",
            "publisher",
            "source_url",
            "source_tier",
            "evidence_type",
            "stance",
            "as_of_date",
            "claim",
            "notes",
            "active",
            "captured_at",
            "category",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "holding_valuations",
        columns: &[
            "holding_id",
            "symbol",
            "quantity",
            "unit_price",
            "market_value",
            "currency",
            "requested_on",
            "observed_on",
            "staleness_days",
            "provider_code",
            "provider_name",
            "exchange_name",
            "mic_code",
            "instrument_type",
            "price_basis",
            "source_url",
            "methodology_url",
            "disclaimer",
            "captured_at",
        ],
        order_by: "holding_id",
    },
    SyncTableSpec {
        name: "daily_settings",
        columns: &["id", "timezone"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "daily_entries",
        columns: &["id", "day", "entity_id", "payload"],
        order_by: "day, entity_id",
    },
    SyncTableSpec {
        name: "settings",
        columns: &["key", "value"],
        order_by: "key",
    },
    SyncTableSpec {
        name: "research_attachments",
        columns: &[
            "id",
            "evidence_id",
            "name",
            "byte_size",
            "data_base64",
            "extracted_text",
            "extraction_note",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "holding_trades",
        columns: &["id", "holding_id", "payload", "created_at"],
        order_by: "created_at, id",
    },
    SyncTableSpec {
        name: "financial_operations",
        columns: &["id", "request_hash", "payload", "created_at"],
        order_by: "created_at,id",
    },
    SyncTableSpec {
        name: "financial_journal",
        columns: &[
            "id",
            "holding_id",
            "kind",
            "before_payload",
            "after_payload",
            "cash_delta",
            "reason",
            "created_at",
            "operation_id",
        ],
        order_by: "created_at,id",
    },
    SyncTableSpec {
        name: "trading_quarantine",
        columns: &["holding_id", "payload", "reason", "created_at"],
        order_by: "holding_id",
    },
    SyncTableSpec {
        name: "cash_state",
        columns: &[
            "currency",
            "holding_id",
            "amount",
            "baseline_id",
            "confirmed_on",
            "confirmed_at",
            "needs_reconciliation",
        ],
        order_by: "currency, holding_id",
    },
    SyncTableSpec {
        name: "corporate_actions",
        columns: &["id", "holding_id", "payload"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "holding_options",
        columns: &["holding_id", "underlying_id", "payload"],
        order_by: "holding_id",
    },
    SyncTableSpec {
        name: "holding_amounts",
        columns: &["holding_id", "payload"],
        order_by: "holding_id",
    },
    SyncTableSpec {
        name: "watch_entries",
        columns: &[
            "symbol",
            "payload",
            "checked_at",
            "checked_for",
            "refresh_failed",
        ],
        order_by: "symbol",
    },
    SyncTableSpec {
        name: "watch_observations",
        columns: &["symbol", "day", "payload", "alert", "read_at"],
        order_by: "symbol,day",
    },
    SyncTableSpec {
        name: "watch_dividends",
        columns: &["symbol", "payload"],
        order_by: "symbol",
    },
    SyncTableSpec {
        name: "analysis_memories",
        columns: &[
            "id",
            "kind",
            "origin",
            "statement",
            "rationale",
            "source_analysis_ids",
            "quote",
            "active",
            "created_at",
            "updated_at",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "daily_review_runs",
        columns: &[
            "id",
            "review_day",
            "trigger",
            "status",
            "payload",
            "created_at",
        ],
        order_by: "review_day, created_at, id",
    },
    SyncTableSpec {
        name: "daily_review_items",
        columns: &[
            "id",
            "run_id",
            "review_day",
            "kind",
            "symbol",
            "importance",
            "payload",
            "read_at",
            "created_at",
        ],
        order_by: "review_day, created_at, id",
    },
];

pub const SYNC_DATASET_SCHEMA_VERSION: u32 = 34;

impl SyncDataset {
    #[cfg(test)]
    pub fn content_hash(&self) -> AppResult<String> {
        Ok(crate::hash_bytes(&serde_json::to_vec(&self.tables)?))
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.schema_version != SYNC_DATASET_SCHEMA_VERSION {
            return Err(AppError::Validation(format!(
                "不支持的数据快照版本 {}，当前仅支持 v{}",
                self.schema_version, SYNC_DATASET_SCHEMA_VERSION
            )));
        }
        chrono::DateTime::parse_from_rfc3339(&self.exported_at)
            .map_err(|_| AppError::Validation("数据快照时间格式无效".into()))?;
        if self.tables.len() != SYNC_TABLES.len() {
            return Err(AppError::Validation("数据快照缺少必要数据表".into()));
        }
        let mut total_rows = 0_usize;
        for spec in SYNC_TABLES {
            let table = self
                .tables
                .iter()
                .find(|table| table.name == spec.name)
                .ok_or_else(|| AppError::Validation(format!("数据快照缺少 {}", spec.name)))?;
            let expected_columns = spec
                .columns
                .iter()
                .copied()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if table.columns != expected_columns {
                return Err(AppError::Validation(format!(
                    "数据表 {} 的字段结构不匹配",
                    spec.name
                )));
            }
            total_rows += table.rows.len();
            if total_rows > 50_000 {
                return Err(AppError::Validation("同步记录超过 50000 条安全上限".into()));
            }
            for row in &table.rows {
                if row.len() != spec.columns.len() {
                    return Err(AppError::Validation(format!(
                        "数据表 {} 存在字段数量错误的记录",
                        spec.name
                    )));
                }
                for (index, value) in row.iter().enumerate() {
                    match value {
                        SyncValue::Real(number) if !number.is_finite() => {
                            return Err(AppError::Validation("同步数据包含无效数字".into()));
                        }
                        SyncValue::Text(text)
                            if text.len()
                                > if spec.name == "research_attachments" && index == 4 {
                                    crate::attachments::MAX_BYTES.div_ceil(3) * 4
                                } else {
                                    2 * 1024 * 1024
                                } =>
                        {
                            return Err(AppError::Validation("同步字段超过大小上限".into()));
                        }
                        _ => {}
                    }
                }
            }
        }
        super::sync_validation::validate_synced_settings(self)?;
        super::fx::validate_sync(self)?;
        super::sync_validation::validate_synced_attachments(self)?;
        Ok(())
    }
}
