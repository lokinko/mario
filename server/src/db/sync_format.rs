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

const LEGACY_HOLDING_COLUMNS: &[&str] = &[
    "id",
    "symbol",
    "name",
    "asset_class",
    "market_value",
    "cost_basis",
    "target_pct",
    "currency",
    "created_at",
    "updated_at",
];

const V3_HOLDING_COLUMNS: &[&str] = &[
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
    "created_at",
    "updated_at",
];

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
];

const LEGACY_EVENT_COLUMNS: &[&str] = &[
    "id",
    "event_type",
    "asset_name",
    "amount",
    "currency",
    "fx_rate_to_base",
    "base_currency",
    "base_amount",
    "occurred_on",
    "note",
    "created_at",
];

const V5_EVENT_COLUMNS: &[&str] = &[
    "id",
    "event_type",
    "source",
    "external_id",
    "fingerprint",
    "asset_name",
    "amount",
    "currency",
    "fx_rate_to_base",
    "base_currency",
    "base_amount",
    "occurred_on",
    "note",
    "created_at",
];

const V6_EVENT_COLUMNS: &[&str] = &[
    "id",
    "event_type",
    "source",
    "external_id",
    "fingerprint",
    "asset_name",
    "amount",
    "currency",
    "fx_rate_to_base",
    "fx_rate_source",
    "fx_rate_observed_on",
    "base_currency",
    "base_amount",
    "occurred_on",
    "note",
    "created_at",
];

const CURRENT_EVENT_COLUMNS: &[&str] = &[
    "id",
    "event_type",
    "source",
    "external_id",
    "fingerprint",
    "asset_name",
    "amount",
    "currency",
    "fx_rate_to_base",
    "fx_rate_source",
    "fx_rate_observed_on",
    "base_currency",
    "base_amount",
    "occurred_on",
    "note",
    "created_at",
    "reversal_of_event_id",
];

const V7_SYNC_TABLE_COUNT: usize = 12;

pub(super) const SYNC_TABLES: &[SyncTableSpec] = &[
    SyncTableSpec {
        name: "profile",
        columns: &["id", "payload", "updated_at"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "goals",
        columns: &[
            "id",
            "name",
            "target_amount",
            "current_amount",
            "monthly_contribution",
            "target_date",
            "priority",
            "created_at",
            "updated_at",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "holdings",
        columns: CURRENT_HOLDING_COLUMNS,
        order_by: "id",
    },
    SyncTableSpec {
        name: "decisions",
        columns: &["id", "asset_name", "payload", "review_date", "created_at"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "decision_reviews",
        columns: &[
            "decision_id",
            "outcome_summary",
            "actual_return_pct",
            "thesis_status",
            "process_rating",
            "lessons",
            "reviewed_at",
        ],
        order_by: "decision_id",
    },
    SyncTableSpec {
        name: "analyses",
        columns: &["id", "question", "answer", "audit", "trace", "created_at"],
        order_by: "id",
    },
    SyncTableSpec {
        name: "system_reviews",
        columns: &[
            "id",
            "payload",
            "snapshot",
            "next_review_date",
            "created_at",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "investment_rules",
        columns: &[
            "id",
            "category",
            "statement",
            "trigger",
            "rationale",
            "active",
            "source_review_id",
            "revision",
            "created_at",
            "updated_at",
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "investment_rule_revisions",
        columns: &["rule_id", "revision", "payload", "changed_at"],
        order_by: "rule_id, revision",
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
        ],
        order_by: "id",
    },
    SyncTableSpec {
        name: "portfolio_checkins",
        columns: &["id", "payload", "created_at"],
        order_by: "created_at, id",
    },
    SyncTableSpec {
        name: "portfolio_events",
        columns: CURRENT_EVENT_COLUMNS,
        order_by: "occurred_on, created_at, id",
    },
    SyncTableSpec {
        name: "memory_preferences",
        columns: &["memory_id", "preference", "note", "updated_at"],
        order_by: "memory_id",
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
];

pub const SYNC_DATASET_SCHEMA_VERSION: u32 = 9;
const V1_SYNC_TABLE_COUNT: usize = 10;
const V2_V3_SYNC_TABLE_COUNT: usize = 11;
const V8_SYNC_TABLE_COUNT: usize = 13;

pub(super) fn sync_specs_for_version(version: u32) -> AppResult<Vec<SyncTableSpec>> {
    let mut specs = SYNC_TABLES.to_vec();
    match version {
        1 => {
            specs.truncate(V1_SYNC_TABLE_COUNT);
            specs[2].columns = LEGACY_HOLDING_COLUMNS;
            Ok(specs)
        }
        2 => {
            specs.truncate(V2_V3_SYNC_TABLE_COUNT);
            specs[2].columns = LEGACY_HOLDING_COLUMNS;
            Ok(specs)
        }
        3 => {
            specs.truncate(V2_V3_SYNC_TABLE_COUNT);
            specs[2].columns = V3_HOLDING_COLUMNS;
            Ok(specs)
        }
        4 => {
            specs.truncate(V7_SYNC_TABLE_COUNT);
            specs[2].columns = V3_HOLDING_COLUMNS;
            specs[11].columns = LEGACY_EVENT_COLUMNS;
            Ok(specs)
        }
        5 => {
            specs.truncate(V7_SYNC_TABLE_COUNT);
            specs[2].columns = V3_HOLDING_COLUMNS;
            specs[11].columns = V5_EVENT_COLUMNS;
            Ok(specs)
        }
        6 => {
            specs.truncate(V7_SYNC_TABLE_COUNT);
            specs[11].columns = V6_EVENT_COLUMNS;
            Ok(specs)
        }
        7 => {
            specs.truncate(V7_SYNC_TABLE_COUNT);
            Ok(specs)
        }
        8 => {
            specs.truncate(V8_SYNC_TABLE_COUNT);
            Ok(specs)
        }
        SYNC_DATASET_SCHEMA_VERSION => Ok(specs),
        _ => Err(AppError::Validation(format!(
            "不支持的数据快照版本 {version}"
        ))),
    }
}

impl SyncDataset {
    pub fn content_hash(&self) -> AppResult<String> {
        Ok(crate::cloud_sync::hash_bytes(&serde_json::to_vec(
            &self.tables,
        )?))
    }

    pub fn record_count(&self) -> usize {
        self.tables.iter().map(|table| table.rows.len()).sum()
    }

    pub fn validate(&self) -> AppResult<()> {
        let specs = sync_specs_for_version(self.schema_version)?;
        chrono::DateTime::parse_from_rfc3339(&self.exported_at)
            .map_err(|_| AppError::Validation("数据快照时间格式无效".into()))?;
        if self.tables.len() != specs.len() {
            return Err(AppError::Validation("数据快照缺少必要数据表".into()));
        }
        let mut total_rows = 0_usize;
        for spec in &specs {
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
                for value in row {
                    match value {
                        SyncValue::Real(number) if !number.is_finite() => {
                            return Err(AppError::Validation("同步数据包含无效数字".into()));
                        }
                        SyncValue::Text(text) if text.len() > 2 * 1024 * 1024 => {
                            return Err(AppError::Validation(
                                "单个同步字段超过 2 MB 安全上限".into(),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}
