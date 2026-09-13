//! Daily observations are authoritative; filled daily totals are a disposable cache.
//! All helpers taking a connection also accept a transaction and never re-lock Database.
use super::*;
use chrono::{DateTime, Duration};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    holding: Option<Holding>,
    removed: bool,
    confirmed_on: Option<String>,
    base_currency: Option<String>,
    liabilities: Option<f64>,
    actual_update: bool,
    #[serde(default)]
    request_id: Option<String>,
}
impl Entry {
    fn empty() -> Self {
        Self {
            holding: None,
            removed: false,
            confirmed_on: None,
            base_currency: None,
            liabilities: None,
            actual_update: false,
            request_id: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyAsset {
    pub holding: Holding,
    pub confirmed_on: Option<String>,
    pub carried: bool,
    pub removed: bool,
    pub base_value: Option<f64>,
    #[serde(default)]
    pub previous_day_change: Option<f64>,
    #[serde(default)]
    pub pct_point_change: Option<f64>,
    #[serde(default)]
    pub change_kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyRecord {
    pub day: String,
    pub base_currency: String,
    pub total_assets: Option<f64>,
    pub liabilities: f64,
    pub net_assets: Option<f64>,
    pub carried: bool,
    pub assets: Vec<DailyAsset>,
    pub missing_fx: Vec<String>,
    pub previous_day_change: Option<f64>,
    pub last_updated_day: Option<String>,
    pub since_last_update_change: Option<f64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyHistory {
    pub timezone: String,
    pub today: String,
    pub records: Vec<DailyRecord>,
    pub next_before: Option<String>,
}
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub before: Option<String>,
    pub limit: Option<usize>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureInput {
    pub timezone: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AmountInput {
    pub amount: f64,
    pub expected_revision: String,
    pub request_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareQuery {
    pub from: String,
    pub to: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetChange {
    pub id: String,
    pub name: String,
    pub status: String,
    pub previous: Option<DailyAsset>,
    pub current: Option<DailyAsset>,
    pub amount_change: Option<f64>,
    pub pct_point_change: Option<f64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyComparison {
    pub from: DailyRecord,
    pub to: DailyRecord,
    pub amount_change: Option<f64>,
    pub assets: Vec<AssetChange>,
    pub allocation_changes: Vec<crate::models::PortfolioAllocationChange>,
}
fn parse_day(value: &str) -> AppResult<NaiveDate> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("日期必须为 YYYY-MM-DD".into()))?;
    if date.to_string() != value {
        return Err(AppError::Validation("日期格式无效".into()));
    }
    Ok(date)
}
fn timezone(conn: &Connection) -> AppResult<String> {
    Ok(conn
        .query_row(
            "SELECT timezone FROM daily_settings WHERE id='account'",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into())))
}
fn today_at(conn: &Connection, now: DateTime<Utc>) -> AppResult<String> {
    let tz: chrono_tz::Tz = timezone(conn)?
        .parse()
        .map_err(|_| AppError::Validation("记账时区无效".into()))?;
    Ok(now.with_timezone(&tz).date_naive().to_string())
}
fn put(conn: &Connection, day: &str, entity: &str, entry: &Entry) -> AppResult<()> {
    conn.execute(
        "INSERT INTO daily_entries(id,day,entity_id,payload) VALUES (?1,?2,?3,?4)
        ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
        params![
            format!("{day}:{entity}"),
            day,
            entity,
            serde_json::to_string(entry)?
        ],
    )?;
    Ok(())
}
fn latest_entry(conn: &Connection, day: &str, entity: &str) -> AppResult<Option<Entry>> {
    conn.query_row("SELECT payload FROM daily_entries WHERE entity_id=?1 AND day<=?2 ORDER BY day DESC LIMIT 1",
        params![entity,day], |r| r.get::<_,String>(0)).optional()?
        .map(|v| serde_json::from_str(&v).map_err(AppError::from)).transpose()
}
fn asset_entry(holding: &Holding, actual: bool, confirmed: Option<String>) -> Entry {
    Entry {
        holding: Some(holding.clone()),
        confirmed_on: confirmed,
        actual_update: actual,
        ..Entry::empty()
    }
}
fn valuation_day(holding: &Holding) -> Option<String> {
    (!holding.valuation_date.is_empty()).then(|| holding.valuation_date.clone())
}
fn initialize_at(conn: &Connection, snapshot: &Snapshot, day: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO daily_settings(id,timezone) VALUES ('account',?1)",
        [timezone(conn)?],
    )?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM daily_entries", [], |r| r.get(0))?;
    if count == 0 {
        put(
            conn,
            day,
            "$base",
            &Entry {
                base_currency: Some(snapshot.profile.base_currency.clone()),
                ..Entry::empty()
            },
        )?;
        put(
            conn,
            day,
            "$liabilities",
            &Entry {
                liabilities: Some(snapshot.profile.liabilities),
                ..Entry::empty()
            },
        )?;
        for h in &snapshot.holdings {
            put(conn, day, &h.id, &asset_entry(h, false, valuation_day(h)))?;
        }
    }
    Ok(())
}
pub(super) fn prepare(conn: &Connection) -> AppResult<Snapshot> {
    prepare_at(conn, Utc::now())
}
fn prepare_at(conn: &Connection, now: DateTime<Utc>) -> AppResult<Snapshot> {
    let snapshot = Database::snapshot_on(conn)?;
    let day = today_at(conn, now)?;
    initialize_at(conn, &snapshot, &day)?;
    let through: Option<String> =
        conn.query_row("SELECT MAX(day) FROM daily_totals", [], |r| r.get(0))?;
    if through.as_deref().is_none_or(|value| value < day.as_str()) {
        rebuild_at(conn, &day)?;
    }
    Ok(snapshot)
}
pub(super) fn finish(
    conn: &Connection,
    before: &Snapshot,
    confirmation: Option<&str>,
) -> AppResult<()> {
    finish_at(conn, before, confirmation, Utc::now())
}
fn finish_at(
    conn: &Connection,
    before: &Snapshot,
    confirmation: Option<&str>,
    now: DateTime<Utc>,
) -> AppResult<()> {
    let after = Database::snapshot_on(conn)?;
    let day = today_at(conn, now)?;
    for h in &after.holdings {
        let old = before.holdings.iter().find(|old| old.id == h.id);
        if old.map(serde_json::to_value).transpose()? != Some(serde_json::to_value(h)?)
            || confirmation == Some(h.id.as_str())
        {
            let confirmed = if confirmation == Some(h.id.as_str()) {
                Some(day.clone())
            } else if old.is_none_or(|old| {
                old.market_value != h.market_value
                    || old.valuation_date != h.valuation_date
                    || old.currency != h.currency
            }) {
                valuation_day(h)
            } else {
                latest_entry(conn, &day, &h.id)?.and_then(|e| e.confirmed_on)
            };
            put(conn, &day, &h.id, &asset_entry(h, true, confirmed))?;
        }
    }
    for h in &before.holdings {
        if !after.holdings.iter().any(|a| a.id == h.id) {
            let mut entry = latest_entry(conn, &day, &h.id)?
                .unwrap_or_else(|| asset_entry(h, false, valuation_day(h)));
            entry.removed = true;
            entry.actual_update = true;
            entry.request_id = None;
            put(conn, &day, &h.id, &entry)?;
        }
    }
    if before.profile.base_currency != after.profile.base_currency {
        put(
            conn,
            &day,
            "$base",
            &Entry {
                base_currency: Some(after.profile.base_currency),
                actual_update: true,
                ..Entry::empty()
            },
        )?;
    }
    if before.profile.liabilities != after.profile.liabilities {
        put(
            conn,
            &day,
            "$liabilities",
            &Entry {
                liabilities: Some(after.profile.liabilities),
                actual_update: true,
                ..Entry::empty()
            },
        )?;
    }
    rebuild_at(conn, &day)
}
fn delta(a: &DailyRecord, b: &DailyRecord) -> Option<f64> {
    if a.base_currency != b.base_currency {
        return None;
    }
    Some(b.total_assets? - a.total_assets?)
}
/// Rebuild from sparse observations, never from the current holdings. Thus a later
/// edit or a synchronized observation cannot leak backwards into earlier dates.
fn rebuild_at(conn: &Connection, through: &str) -> AppResult<()> {
    let mut statement =
        conn.prepare("SELECT day,entity_id,payload FROM daily_entries ORDER BY day,entity_id")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    conn.execute("DELETE FROM daily_totals", [])?;
    let Some(first) = rows.first() else {
        return Ok(());
    };
    let mut day = parse_day(&first.0)?;
    let end = parse_day(through)?.max(parse_day(&rows.last().unwrap().0)?);
    if (end - day).num_days() > 36600 {
        return Err(AppError::Validation("每日历史跨度超过 100 年".into()));
    }
    let mut state = BTreeMap::<String, (String, Entry)>::new();
    let mut index = 0;
    let mut previous: Option<DailyRecord> = None;
    let mut last_actual: Option<DailyRecord> = None;
    while day <= end {
        let key = day.to_string();
        let mut actual = false;
        while index < rows.len() && rows[index].0 == key {
            let (_, entity, payload) = &rows[index];
            let entry: Entry = serde_json::from_str(payload)?;
            actual |= entry.actual_update;
            state.insert(entity.clone(), (key.clone(), entry));
            index += 1;
        }
        let base = state
            .get("$base")
            .and_then(|(_, e)| e.base_currency.clone())
            .ok_or_else(|| AppError::Validation("每日历史缺少基准币种".into()))?;
        let liabilities = state
            .get("$liabilities")
            .and_then(|(_, e)| e.liabilities)
            .ok_or_else(|| AppError::Validation("每日历史缺少负债状态".into()))?;
        let assets = state
            .values()
            .filter_map(|(recorded, e)| {
                let holding = e.holding.clone()?;
                // Keep tombstones on the removal date for the history detail.
                if e.removed && recorded != &key {
                    return None;
                }
                let base_value = if e.removed {
                    Some(0.0)
                } else {
                    valuation::normalize_holding(&holding, &base)
                        .map(|h| h.market_value)
                        .filter(|v| v.is_finite())
                };
                Some(DailyAsset {
                    holding,
                    confirmed_on: e.confirmed_on.clone(),
                    carried: e.confirmed_on.as_deref() != Some(key.as_str()),
                    removed: e.removed,
                    base_value,
                    previous_day_change: None,
                    pct_point_change: None,
                    change_kind: "baseline".into(),
                })
            })
            .collect::<Vec<_>>();
        let missing_fx = assets
            .iter()
            .filter(|a| a.base_value.is_none())
            .map(|a| a.holding.name.clone())
            .collect::<Vec<_>>();
        let total = missing_fx
            .is_empty()
            .then(|| assets.iter().filter_map(|a| a.base_value).sum::<f64>())
            .filter(|v| v.is_finite());
        let mut record = DailyRecord {
            day: key.clone(),
            base_currency: base,
            total_assets: total,
            liabilities,
            net_assets: total.map(|v| v - liabilities),
            carried: !actual,
            assets,
            missing_fx,
            previous_day_change: None,
            last_updated_day: None,
            since_last_update_change: None,
        };
        if let Some(prev) = &previous {
            record.previous_day_change = delta(prev, &record);
            for asset in &mut record.assets {
                let prior = prev
                    .assets
                    .iter()
                    .find(|a| a.holding.id == asset.holding.id && !a.removed);
                asset.change_kind = if asset.removed {
                    "removed"
                } else if prior.is_none() {
                    "added"
                } else {
                    "existing"
                }
                .into();
                if prev.base_currency == record.base_currency {
                    let previous_value = prior.map_or(Some(0.0), |a| a.base_value);
                    asset.previous_day_change =
                        asset.base_value.zip(previous_value).map(|(b, a)| b - a);
                    asset.pct_point_change = prev
                        .total_assets
                        .zip(record.total_assets)
                        .filter(|(a, b)| *a > 0.0 && *b > 0.0)
                        .and_then(|(a, b)| {
                            previous_value
                                .zip(asset.base_value)
                                .map(|(av, bv)| (bv / b - av / a) * 100.0)
                        });
                }
            }
        }
        if let Some(last) = &last_actual {
            record.last_updated_day = Some(last.day.clone());
            record.since_last_update_change = delta(last, &record);
        }
        conn.execute(
            "INSERT INTO daily_totals(day,payload) VALUES (?1,?2)",
            params![key, serde_json::to_string(&record)?],
        )?;
        if actual || last_actual.is_none() {
            last_actual = Some(record.clone());
        }
        previous = Some(record);
        day += Duration::days(1);
    }
    Ok(())
}
pub(super) fn rebuild(conn: &Connection) -> AppResult<()> {
    rebuild_at(conn, &today_at(conn, Utc::now())?)
}
impl Database {
    pub fn ensure_daily(&self, input: &EnsureInput) -> AppResult<DailyHistory> {
        input
            .timezone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| AppError::Validation("记账时区无效".into()))?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO daily_settings(id,timezone) VALUES ('account',?1)",
            [&input.timezone],
        )?;
        prepare(&tx)?;
        tx.commit()?;
        drop(conn);
        self.daily_history(&HistoryQuery {
            limit: Some(30),
            ..Default::default()
        })
    }
    pub fn daily_history(&self, query: &HistoryQuery) -> AppResult<DailyHistory> {
        for d in [&query.from, &query.to, &query.before]
            .into_iter()
            .flatten()
        {
            parse_day(d)?;
        }
        if query
            .from
            .as_ref()
            .zip(query.to.as_ref())
            .is_some_and(|(f, t)| f > t)
        {
            return Err(AppError::Validation("开始日期不能晚于结束日期".into()));
        }
        let limit = query.limit.unwrap_or(30).clamp(1, 400);
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT payload FROM daily_totals WHERE (?1 IS NULL OR day>=?1) AND (?2 IS NULL OR day<=?2) AND (?3 IS NULL OR day<?3) ORDER BY day DESC LIMIT ?4")?;
        let mut records = stmt
            .query_map(
                params![query.from, query.to, query.before, limit + 1],
                |r| r.get::<_, String>(0),
            )?
            .map(|row| Ok(serde_json::from_str::<DailyRecord>(&row?)?))
            .collect::<AppResult<Vec<_>>>()?;
        let next_before = (records.len() > limit).then(|| records[limit - 1].day.clone());
        records.truncate(limit);
        Ok(DailyHistory {
            timezone: timezone(&conn)?,
            today: today_at(&conn, Utc::now())?,
            records,
            next_before,
        })
    }
    pub fn daily_compare(&self, query: &CompareQuery) -> AppResult<DailyComparison> {
        parse_day(&query.from)?;
        parse_day(&query.to)?;
        if query.from > query.to {
            return Err(AppError::Validation("开始日期不能晚于结束日期".into()));
        }
        let conn = self.conn()?;
        let read = |day: &str| -> AppResult<DailyRecord> {
            let payload = conn
                .query_row(
                    "SELECT payload FROM daily_totals WHERE day=?1",
                    [day],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| AppError::Validation("该日期尚无资产记录".into()))?;
            Ok(serde_json::from_str(&payload)?)
        };
        Ok(compare(read(&query.from)?, read(&query.to)?))
    }
    pub fn update_holding_amount(&self, id: &str, input: &AmountInput) -> AppResult<Snapshot> {
        validate_non_negative(&[input.amount])?;
        if input.amount > 1e15 {
            return Err(AppError::Validation("金额超过允许范围".into()));
        }
        if input.request_id.is_empty() || input.request_id.len() > 128 {
            return Err(AppError::Validation("保存请求编号无效".into()));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let now = Utc::now();
        let before = prepare_at(&tx, now)?;
        let holding = before
            .holdings
            .iter()
            .find(|h| h.id == id)
            .ok_or_else(|| AppError::Validation("找不到要更新的资产".into()))?;
        let day = today_at(&tx, now)?;
        if latest_entry(&tx, &day, id)?
            .is_some_and(|e| e.request_id.as_deref() == Some(&input.request_id))
            && holding.market_value == input.amount
        {
            tx.commit()?;
            drop(conn);
            return self.snapshot();
        }
        if before.holding_revisions.get(id) != Some(&input.expected_revision) {
            return Err(AppError::Conflict(
                "该资产已更新，请核对最新金额后重试；输入已保留".into(),
            ));
        }
        // FX provenance has its own observation date; manual amounts do not refresh it.
        tx.execute(
            "UPDATE holdings SET market_value=?2, valuation_date=?3, updated_at=?4 WHERE id=?1",
            params![id, input.amount, day, Utc::now().to_rfc3339()],
        )?;
        tx.execute("DELETE FROM holding_valuations WHERE holding_id=?1", [id])?;
        finish_at(&tx, &before, Some(id), now)?;
        let mut entry = latest_entry(&tx, &day, id)?.unwrap();
        entry.request_id = Some(input.request_id.clone());
        put(&tx, &day, id, &entry)?;
        tx.commit()?;
        drop(conn);
        self.snapshot()
    }
}
pub fn compare(from: DailyRecord, to: DailyRecord) -> DailyComparison {
    let compatible = from.base_currency == to.base_currency;
    let left = from
        .assets
        .iter()
        .filter(|a| !a.removed)
        .map(|a| (a.holding.id.clone(), a))
        .collect::<BTreeMap<_, _>>();
    let right = to
        .assets
        .iter()
        .filter(|a| !a.removed)
        .map(|a| (a.holding.id.clone(), a))
        .collect::<BTreeMap<_, _>>();
    let keys = left
        .keys()
        .chain(right.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut assets = keys
        .into_iter()
        .map(|id| {
            let a = left.get(&id).copied();
            let b = right.get(&id).copied();
            let av = a.map_or(Some(0.0), |a| a.base_value);
            let bv = b.map_or(Some(0.0), |b| b.base_value);
            let amount_change = compatible.then(|| bv.zip(av).map(|(b, a)| b - a)).flatten();
            let pct_point_change = if compatible {
                from.total_assets
                    .zip(to.total_assets)
                    .filter(|(a, b)| *a > 0.0 && *b > 0.0)
                    .and_then(|(at, bt)| av.zip(bv).map(|(a, b)| (b / bt - a / at) * 100.0))
            } else {
                None
            };
            AssetChange {
                id,
                name: b.or(a).unwrap().holding.name.clone(),
                status: if a.is_none() {
                    "added"
                } else if b.is_none() {
                    "removed"
                } else {
                    "existing"
                }
                .into(),
                previous: a.cloned(),
                current: b.cloned(),
                amount_change,
                pct_point_change,
            }
        })
        .collect::<Vec<_>>();
    assets.sort_by(|a, b| {
        b.amount_change
            .unwrap_or(0.0)
            .abs()
            .total_cmp(&a.amount_change.unwrap_or(0.0).abs())
    });
    let allocation_changes =
        if compatible && from.total_assets.is_some() && to.total_assets.is_some() {
            valuation::allocation_changes(
                &left.values().map(|a| a.holding.clone()).collect::<Vec<_>>(),
                &right
                    .values()
                    .map(|a| a.holding.clone())
                    .collect::<Vec<_>>(),
                &to.base_currency,
            )
        } else {
            vec![]
        };
    DailyComparison {
        amount_change: delta(&from, &to),
        from,
        to,
        assets,
        allocation_changes,
    }
}
/// Import validation also runs inside a temporary database before a cloud write.
pub(super) fn validate(conn: &Connection) -> AppResult<()> {
    let settings: i64 = conn.query_row("SELECT COUNT(*) FROM daily_settings", [], |r| r.get(0))?;
    if settings > 1 {
        return Err(AppError::Validation("每日记账设置重复".into()));
    }
    let mut stmt = conn.prepare("SELECT id,timezone FROM daily_settings")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, tz) = row?;
        if id != "account" || tz.parse::<chrono_tz::Tz>().is_err() {
            return Err(AppError::Validation("每日记账设置无效".into()));
        }
    }
    let mut stmt = conn.prepare("SELECT id,day,entity_id,payload FROM daily_entries")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (id, day, entity, payload) = row?;
        let date = parse_day(&day)?;
        let entry: Entry = serde_json::from_str(&payload)?;
        if settings != 1
            || id != format!("{day}:{entity}")
            || date > parse_day(&today_at(conn, Utc::now())?)?
            || date < parse_day("1970-01-01")?
        {
            return Err(AppError::Validation("每日资产标识或日期无效".into()));
        }
        if let Some(confirmed) = &entry.confirmed_on {
            parse_day(confirmed)?;
        }
        match entity.as_str() {
            "$base"
                if entry.holding.is_none()
                    && entry.liabilities.is_none()
                    && entry.base_currency.is_some() =>
            {
                validate_currency(entry.base_currency.as_deref().unwrap())?
            }
            "$liabilities"
                if entry.holding.is_none()
                    && entry.base_currency.is_none()
                    && entry.liabilities.is_some() =>
            {
                validate_non_negative(&[entry.liabilities.unwrap()])?
            }
            _ if entry.base_currency.is_none()
                && entry.liabilities.is_none()
                && entry
                    .holding
                    .as_ref()
                    .is_some_and(|h| h.id == entity && !entity.starts_with('$')) =>
            {
                let h = entry.holding.unwrap();
                validate_non_negative(&[h.market_value, h.cost_basis])?;
                validate_currency(&h.currency)?;
                if h.fx_rate_to_base
                    .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 1e15)
                {
                    return Err(AppError::Validation("历史汇率无效".into()));
                }
            }
            _ => return Err(AppError::Validation("每日资产内容无效".into())),
        }
    }
    rebuild(conn)
}

impl Database {
    pub fn daily_analysis_context(
        &self,
        request: &crate::models::AnalysisRequest,
    ) -> AppResult<serde_json::Value> {
        if !request.context_selection.include_portfolio_checkins {
            return Ok(serde_json::Value::Null);
        }
        let history = self.daily_history(&HistoryQuery {
            limit: Some(30),
            ..Default::default()
        })?;
        let comparison = if let Some(range) = &request.daily_asset_range {
            Some(self.daily_compare(range)?)
        } else if history.records.len() > 1 {
            Some(compare(
                history.records.last().unwrap().clone(),
                history.records[0].clone(),
            ))
        } else {
            None
        };
        let mut result = serde_json::json!({"history":history,"comparison":comparison,
            "meaning":"每日已录入资产金额；carried 为沿用值，不是该日重新估值。金额差额不能证明收益、买卖或投资能力。流水可能不完整。"});
        if !request.context_selection.include_profile {
            redact_profile(&mut result);
        }
        Ok(result)
    }
}
fn redact_profile(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("liabilities");
            map.remove("netAssets");
            for child in map.values_mut() {
                redact_profile(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                redact_profile(child);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Database {
        Database::open(std::path::Path::new(":memory:")).unwrap()
    }
    fn input(name: &str, value: f64) -> HoldingInput {
        serde_json::from_value(serde_json::json!({"symbol":"","name":name,"assetClass":"现金","marketValue":value,"costBasis":0,"targetPct":0,"currency":"CNY","fxRateToBase":null,"valuationDate":"2026-01-01","fxRateSource":"","fxRateObservedOn":""})).unwrap()
    }
    fn amount(db: &Database, id: &str, value: f64) -> Snapshot {
        let revision = db.snapshot().unwrap().holding_revisions[id].clone();
        db.update_holding_amount(
            id,
            &AmountInput {
                amount: value,
                expected_revision: revision,
                request_id: Uuid::new_v4().to_string(),
            },
        )
        .unwrap()
    }
    fn records(db: &Database) -> Vec<DailyRecord> {
        db.daily_history(&HistoryQuery {
            limit: Some(400),
            ..Default::default()
        })
        .unwrap()
        .records
    }
    fn instant(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }
    fn seed_history(db: &Database, when: &str) {
        let c = db.conn().unwrap();
        c.execute("DELETE FROM daily_entries", []).unwrap();
        c.execute("DELETE FROM daily_totals", []).unwrap();
        c.execute("INSERT INTO daily_settings(id,timezone) VALUES ('account','Asia/Shanghai') ON CONFLICT(id) DO UPDATE SET timezone=excluded.timezone",[]).unwrap();
        prepare_at(&c, instant(when)).unwrap();
    }
    fn change_at(db: &Database, id: &str, value: f64, when: &str) {
        let mut c = db.conn().unwrap();
        let tx = c.transaction().unwrap();
        let now = instant(when);
        let before = prepare_at(&tx, now).unwrap();
        tx.execute(
            "UPDATE holdings SET market_value=?2 WHERE id=?1",
            params![id, value],
        )
        .unwrap();
        finish_at(&tx, &before, Some(id), now).unwrap();
        tx.commit().unwrap();
    }
    #[test]
    fn daily_upsert_zero_idempotency_stale_write_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daily.db");
        let db = Database::open(&path).unwrap();
        let id = db.add_holding(&input("银行", 100.0)).unwrap().holdings[0]
            .id
            .clone();
        let old_revision = db.snapshot().unwrap().holding_revisions[&id].clone();
        for n in 0..10 {
            amount(&db, &id, n as f64);
        }
        assert_eq!(records(&db).len(), 1);
        assert_eq!(
            db.conn()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM daily_entries WHERE entity_id=?1",
                    [&id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        let req = AmountInput {
            amount: 0.0,
            expected_revision: db.snapshot().unwrap().holding_revisions[&id].clone(),
            request_id: "retry".into(),
        };
        db.update_holding_amount(&id, &req).unwrap();
        let before = db.export_sync_data().unwrap().content_hash().unwrap();
        db.update_holding_amount(&id, &req).unwrap();
        assert_eq!(
            before,
            db.export_sync_data().unwrap().content_hash().unwrap()
        );
        assert!(matches!(
            db.update_holding_amount(
                &id,
                &AmountInput {
                    amount: 800.0,
                    expected_revision: old_revision,
                    request_id: "stale".into()
                }
            ),
            Err(AppError::Conflict(_))
        ));
        for invalid in [-1.0, f64::NAN, f64::INFINITY, 1e16] {
            assert!(db
                .update_holding_amount(
                    &id,
                    &AmountInput {
                        amount: invalid,
                        expected_revision: req.expected_revision.clone(),
                        request_id: "bad".into()
                    }
                )
                .is_err());
        }
        drop(db);
        let reopened = Database::open(&path).unwrap();
        assert_eq!(records(&reopened)[0].total_assets, Some(0.0));
    }
    #[test]
    fn gaps_use_old_balances_and_only_changed_assets_are_confirmed() {
        let db = db();
        let a = db.add_holding(&input("A", 100.0)).unwrap().holdings[0]
            .id
            .clone();
        let b = db
            .add_holding(&input("B", 200.0))
            .unwrap()
            .holdings
            .iter()
            .find(|h| h.name == "B")
            .unwrap()
            .id
            .clone();
        seed_history(&db, "2026-01-30T15:00:00Z");
        change_at(&db, &a, 150.0, "2026-02-02T16:00:00Z"); // Shanghai Feb 3
        let rows = records(&db);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].day, "2026-02-03");
        assert_eq!(rows[0].total_assets, Some(350.0));
        assert_eq!(rows[0].previous_day_change, Some(50.0));
        assert_eq!(rows[0].since_last_update_change, Some(50.0));
        assert!(rows[1..]
            .iter()
            .all(|r| r.total_assets == Some(300.0) && r.carried));
        assert_eq!(
            rows[0]
                .assets
                .iter()
                .find(|v| v.holding.id == a)
                .unwrap()
                .confirmed_on
                .as_deref(),
            Some("2026-02-03")
        );
        assert_eq!(
            rows[0]
                .assets
                .iter()
                .find(|v| v.holding.id == b)
                .unwrap()
                .confirmed_on
                .as_deref(),
            Some("2026-01-01")
        );
        let comparison = compare(rows[4].clone(), rows[0].clone());
        assert_eq!(comparison.assets[0].amount_change, Some(50.0));
        let page = db
            .daily_history(&HistoryQuery {
                limit: Some(2),
                ..Default::default()
            })
            .unwrap();
        let next = db
            .daily_history(&HistoryQuery {
                limit: Some(2),
                before: page.next_before,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(next.records[0].day, "2026-02-01");
    }
    #[test]
    fn fixed_timezone_rolls_over_year_and_ignores_another_device_timezone() {
        let db = db();
        db.ensure_daily(&EnsureInput {
            timezone: "Asia/Shanghai".into(),
        })
        .unwrap();
        assert_eq!(
            db.ensure_daily(&EnsureInput {
                timezone: "America/New_York".into()
            })
            .unwrap()
            .timezone,
            "Asia/Shanghai"
        );
        let c = db.conn().unwrap();
        assert_eq!(
            today_at(&c, instant("2025-12-31T16:00:00Z")).unwrap(),
            "2026-01-01"
        );
        c.execute("UPDATE daily_settings SET timezone='America/New_York'", [])
            .unwrap();
        assert_eq!(
            today_at(&c, instant("2026-03-08T06:59:00Z")).unwrap(),
            "2026-03-08"
        );
        assert_eq!(
            today_at(&c, instant("2026-03-08T07:01:00Z")).unwrap(),
            "2026-03-08"
        );
    }
    #[test]
    fn asset_write_rolls_back_when_daily_storage_fails() {
        let db = db();
        db.ensure_daily(&EnsureInput {
            timezone: "UTC".into(),
        })
        .unwrap();
        db.conn().unwrap().execute_batch("CREATE TRIGGER fail_daily BEFORE INSERT ON daily_entries BEGIN SELECT RAISE(ABORT,'disk failure'); END;").unwrap();
        assert!(db.add_holding(&input("银行", 100.0)).is_err());
        assert!(db.snapshot().unwrap().holdings.is_empty());
        assert_eq!(records(&db)[0].total_assets, Some(0.0));
    }
    #[test]
    fn missing_fx_liabilities_currency_changes_and_removal_are_honest() {
        let db = db();
        let mut foreign = input("外币", 100.0);
        foreign.currency = "USD".into();
        let id = db.add_holding(&foreign).unwrap().holdings[0].id.clone();
        assert_eq!(records(&db)[0].total_assets, None);
        let restored = super::tests::db();
        restored
            .import_sync_data(&db.export_sync_data().unwrap())
            .unwrap();
        assert_eq!(records(&restored)[0].total_assets, None);
        foreign.fx_rate_to_base = Some(7.0);
        db.update_holding(&id, &foreign).unwrap();
        let before = records(&db)[0].clone();
        let mut profile = db.snapshot().unwrap().profile;
        profile.liabilities = 20.0;
        db.save_profile(&profile).unwrap();
        assert_eq!(records(&db)[0].net_assets, Some(680.0));
        profile.base_currency = "USD".into();
        db.save_profile(&profile).unwrap();
        assert!(compare(before, records(&db)[0].clone())
            .amount_change
            .is_none());
        db.delete_holding(&id).unwrap();
        assert!(records(&db)[0].assets[0].removed);
        assert_eq!(records(&db)[0].total_assets, Some(0.0));
    }
    #[test]
    fn income_edits_do_not_confirm_assets_or_create_observations() {
        let db = db();
        db.add_holding(&input("银行", 100.0)).unwrap();
        let before = db
            .export_sync_data()
            .unwrap()
            .tables
            .into_iter()
            .find(|t| t.name == "daily_entries")
            .unwrap();
        let mut profile = db.snapshot().unwrap().profile;
        profile.monthly_income = 5000.0;
        db.save_profile(&profile).unwrap();
        let after = db
            .export_sync_data()
            .unwrap()
            .tables
            .into_iter()
            .find(|t| t.name == "daily_entries")
            .unwrap();
        assert_eq!(before, after);
    }
    #[test]
    fn independent_daily_edits_merge_and_conflicting_edits_remain_visible() {
        let source = db();
        let a = source.add_holding(&input("A", 100.0)).unwrap().holdings[0]
            .id
            .clone();
        let b = source
            .add_holding(&input("B", 200.0))
            .unwrap()
            .holdings
            .iter()
            .find(|h| h.name == "B")
            .unwrap()
            .id
            .clone();
        let base = source.export_sync_data().unwrap();
        let left = db();
        let right = db();
        left.import_sync_data(&base).unwrap();
        right.import_sync_data(&base).unwrap();
        amount(&left, &a, 150.0);
        amount(&right, &b, 250.0);
        let merged = Database::merge_sync_data(
            Some(&base),
            &left.export_sync_data().unwrap(),
            &right.export_sync_data().unwrap(),
        )
        .unwrap();
        source.apply_sync_update(&merged, &[]).unwrap();
        assert_eq!(records(&source)[0].total_assets, Some(400.0));
        amount(&right, &a, 99.0);
        assert!(matches!(
            Database::merge_sync_data(
                Some(&base),
                &left.export_sync_data().unwrap(),
                &right.export_sync_data().unwrap()
            ),
            Err(AppError::Conflict(_))
        ));
    }
    #[test]
    fn legacy_v9_and_invalid_daily_payloads_are_handled_atomically() {
        let db = db();
        db.add_holding(&input("银行", 100.0)).unwrap();
        let data = db.export_sync_data().unwrap();
        let mut legacy = data.clone();
        legacy.schema_version = 9;
        legacy.tables.truncate(14);
        let restored = super::tests::db();
        restored.import_sync_data(&legacy).unwrap();
        assert!(records(&restored).is_empty());
        restored
            .ensure_daily(&EnsureInput {
                timezone: "UTC".into(),
            })
            .unwrap();
        assert_eq!(
            records(&restored)[0].assets[0].confirmed_on.as_deref(),
            Some("2026-01-01")
        );
        let mut bad = data;
        bad.tables
            .iter_mut()
            .find(|t| t.name == "daily_entries")
            .unwrap()
            .rows[0][3] = SyncValue::Text("{}".into());
        let before = restored.export_sync_data().unwrap().content_hash().unwrap();
        assert!(restored.import_sync_data(&bad).is_err());
        assert_eq!(
            before,
            restored.export_sync_data().unwrap().content_hash().unwrap()
        );
    }
    #[test]
    fn daily_context_obeys_both_authorizations_and_changes_preview_revision() {
        let db = db();
        let id = db.add_holding(&input("银行", 100.0)).unwrap().holdings[0]
            .id
            .clone();
        let mut request:crate::models::AnalysisRequest=serde_json::from_value(serde_json::json!({"question":"资产变化","workflow":"quick","useMemory":false,"reflect":false,"exploreAlternatives":false})).unwrap();
        let context = db.daily_analysis_context(&request).unwrap();
        assert!(context.to_string().contains("netAssets"));
        let snap = db.snapshot().unwrap();
        let built = crate::context::ContextBuilder::build(
            &request,
            &snap,
            &crate::context::ContextSources {
                daily_assets: Some(&context),
                ..Default::default()
            },
        );
        amount(&db, &id, 120.0);
        let next = db.daily_analysis_context(&request).unwrap();
        let changed = crate::context::ContextBuilder::build(
            &request,
            &snap,
            &crate::context::ContextSources {
                daily_assets: Some(&next),
                ..Default::default()
            },
        );
        assert_ne!(built.revision, changed.revision);
        request.context_selection.include_profile = false;
        let redacted = db.daily_analysis_context(&request).unwrap().to_string();
        assert!(!redacted.contains("liabilities"));
        assert!(!redacted.contains("netAssets"));
        request.context_selection.include_portfolio_checkins = false;
        request.daily_asset_range = Some(CompareQuery {
            from: "invalid".into(),
            to: "invalid".into(),
        });
        assert!(db.daily_analysis_context(&request).unwrap().is_null());
    }
}
