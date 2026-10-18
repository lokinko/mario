use super::{
    build_snapshot, normalized_holding_fx_provenance, params, validate_currency, validate_holding,
    validate_holding_valuation_evidence, validate_non_negative, AppError, AppResult, Database,
    FinancialProfile, Holding, HoldingInput, HoldingValuationEvidence, OptionalRow,
    SecurityPriceQuote, Snapshot, Utc, Uuid,
};

impl Database {
    pub fn snapshot(&self) -> AppResult<Snapshot> {
        let conn = self.conn()?;
        Self::snapshot_on(&conn)
    }

    pub(super) fn snapshot_on(conn: &super::Connection) -> AppResult<Snapshot> {
        let profile = conn
            .query_row("SELECT payload FROM profile WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default();

        let mut holding_stmt = conn.prepare("SELECT id, symbol, name, asset_class, market_value, cost_basis, target_pct, currency, fx_rate_to_base, valuation_date, fx_rate_source, fx_rate_observed_on, cost_price, quantity, current_price FROM holdings ORDER BY created_at")?;
        let holdings = holding_stmt
            .query_map([], |row| {
                Ok(Holding {
                    id: row.get(0)?,
                    symbol: row.get(1)?,
                    name: row.get(2)?,
                    asset_class: row.get(3)?,
                    market_value: row.get(4)?,
                    cost_basis: row.get(5)?,
                    cost_price: row.get(12)?,
                    quantity: row.get(13)?,
                    current_price: row.get(14)?,
                    target_pct: row.get(6)?,
                    currency: row.get(7)?,
                    fx_rate_to_base: row.get(8)?,
                    valuation_date: row.get(9)?,
                    fx_rate_source: row.get(10)?,
                    fx_rate_observed_on: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut valuation_stmt = conn.prepare(
            "SELECT holding_id, symbol, quantity, unit_price, market_value, currency,
                    requested_on, observed_on, staleness_days, provider_code, provider_name,
                    exchange_name, mic_code, instrument_type, price_basis, source_url,
                    methodology_url, disclaimer, captured_at
             FROM holding_valuations ORDER BY holding_id",
        )?;
        let holding_valuations = valuation_stmt
            .query_map([], |row| {
                Ok(HoldingValuationEvidence {
                    holding_id: row.get(0)?,
                    symbol: row.get(1)?,
                    quantity: row.get(2)?,
                    unit_price: row.get(3)?,
                    market_value: row.get(4)?,
                    currency: row.get(5)?,
                    requested_on: row.get(6)?,
                    observed_on: row.get(7)?,
                    staleness_days: row.get(8)?,
                    provider_code: row.get(9)?,
                    provider_name: row.get(10)?,
                    exchange: row.get(11)?,
                    mic_code: row.get(12)?,
                    instrument_type: row.get(13)?,
                    price_basis: row.get(14)?,
                    source_url: row.get(15)?,
                    methodology_url: row.get(16)?,
                    disclaimer: row.get(17)?,
                    captured_at: row.get(18)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(valuation_stmt);
        drop(holding_stmt);

        let updated_at = conn
            .query_row(
                "SELECT MAX(value) FROM (
                   SELECT updated_at AS value FROM profile
                   UNION ALL
                   SELECT CASE WHEN updated_at='' THEN created_at ELSE updated_at END FROM holdings
                 )",
                [],
                |row| row.get::<_, Option<String>>(0),
            )?
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        let mut snapshot = build_snapshot(profile, holdings, holding_valuations, updated_at);
        let mut statement = conn.prepare(
            "SELECT id, CASE WHEN updated_at='' THEN created_at ELSE updated_at END FROM holdings",
        )?;
        snapshot.holding_revisions = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        snapshot.holding_trades = super::trades::load(conn)?;
        snapshot.cash_balances = super::cash::states(conn)?;
        snapshot.holding_trade_summaries =
            super::trades::summaries(conn, &snapshot.holding_trades)?;
        snapshot.corporate_actions = super::corporate::load(conn)?;
        if snapshot
            .corporate_actions
            .iter()
            .any(|a| !a.applied && !a.dismissed)
        {
            snapshot.valuation_status.comparable = false;
            snapshot.valuation_status.decision_ready = false;
            snapshot
                .valuation_status
                .warnings
                .push("存在待确认公司行动，相关估值和调仓行动暂停".into());
            snapshot.plan.rebalancing.clear();
        }
        let mut notices =
            conn.prepare("SELECT holding_id FROM trading_quarantine ORDER BY holding_id")?;
        snapshot.cleared_trade_holdings = notices
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if snapshot
            .cash_balances
            .iter()
            .any(|b| b.needs_reconciliation)
        {
            snapshot.valuation_status.decision_ready = false;
            snapshot
                .valuation_status
                .warnings
                .push("存款余额待重新核对".into());
            snapshot.plan.rebalancing.clear();
            snapshot.findings.push(crate::models::RiskFinding {
                level: "high".into(),
                title: "存款余额待核对".into(),
                detail: "核对基准之前的交易发生了更正，当前余额保留，行动建议暂停".into(),
                action: "按银行或券商实际余额重新确认存款".into(),
            });
        }
        if !snapshot.cleared_trade_holdings.is_empty() {
            snapshot.valuation_status.comparable = false;
            snapshot.valuation_status.decision_ready = false;
            snapshot
                .valuation_status
                .warnings
                .push("交易链存在异常，原始记录已隔离，组合估值和行动建议暂停".into());
            snapshot.plan.rebalancing.clear();
            snapshot.plan.assumptions =
                "存在待修复交易链，禁止将缓存数值作为可信资产或行动依据".into();
            snapshot.findings.push(crate::models::RiskFinding {
                level: "high".into(),
                title: "交易原始记录待修复".into(),
                detail: "原始记录和现金余额均已保留，未自动删除或重新结算".into(),
                action: "在交易明细中预览和修复异常记录，并核对存款".into(),
            });
        }
        let accounting = super::amounts::build(conn, &snapshot)?;
        let today =
            chrono::NaiveDate::parse_from_str(&crate::models::market::today(), "%Y-%m-%d").unwrap();
        for evidence in &snapshot.holding_valuations {
            if !chrono::NaiveDate::parse_from_str(&evidence.observed_on, "%Y-%m-%d")
                .is_ok_and(|day| (0..=7).contains(&(today - day).num_days()))
            {
                snapshot.valuation_status.decision_ready = false;
                snapshot.valuation_status.warnings.push(format!(
                    "{} 的实际价格观察日缺失或已超过 7 天",
                    evidence.symbol
                ));
            }
        }
        for holding in &snapshot.holdings {
            if holding.asset_class != "现金"
                && matches!(
                    accounting.holdings[&holding.id]
                        .amounts
                        .price_status
                        .as_str(),
                    "unconfirmed" | "provisional" | "corporate_pending_price"
                )
            {
                snapshot.valuation_status.decision_ready = false;
                snapshot.valuation_status.warnings.push(format!(
                    "{} 当前价格尚未确认，成交参考价不能作为决策估值",
                    holding.name
                ));
            }
        }
        use rust_decimal::{prelude::ToPrimitive, Decimal};
        let number = |value: &str| -> AppResult<f64> {
            Decimal::from_str_exact(value)
                .map_err(|_| AppError::Validation("会计投影金额无效".into()))?
                .to_f64()
                .ok_or_else(|| AppError::Validation("会计投影超出范围".into()))
        };
        for holding in &mut snapshot.holdings {
            let exact = &accounting.holdings[&holding.id];
            if exact.usable {
                holding.market_value = number(&exact.amounts.market_value)?;
                holding.cost_basis = number(&exact.amounts.cost_basis)?;
                holding.quantity = exact.amounts.quantity.as_deref().map(number).transpose()?;
                holding.cost_price = exact
                    .amounts
                    .cost_price
                    .as_deref()
                    .map(number)
                    .transpose()?;
                holding.current_price = exact
                    .amounts
                    .current_price
                    .as_deref()
                    .map(number)
                    .transpose()?;
            }
        }
        let derived = build_snapshot(
            snapshot.profile.clone(),
            snapshot.holdings.clone(),
            snapshot.holding_valuations.clone(),
            snapshot.updated_at.clone(),
        );
        let extra = snapshot
            .findings
            .into_iter()
            .filter(|f| matches!(f.title.as_str(), "存款余额待核对" | "交易原始记录待修复"))
            .collect::<Vec<_>>();
        snapshot.findings = derived.findings;
        snapshot.findings.extend(extra);
        snapshot.plan = derived.plan;
        if let Some(total) = &accounting.total_value {
            let total = Decimal::from_str_exact(total)
                .map_err(|_| AppError::Validation("组合合计无效".into()))?;
            let mut target_sum = Decimal::ZERO;
            for holding in &snapshot.holdings {
                target_sum += Decimal::from_str_exact(&holding.target_pct.to_string())
                    .map_err(|_| AppError::Validation("目标权重无效".into()))?;
            }
            if (target_sum - Decimal::from(100)).abs() > Decimal::new(1, 8) {
                snapshot.plan.rebalancing.clear();
            }
            for action in &mut snapshot.plan.rebalancing {
                let source = &accounting.holdings[&action.holding_id];
                let weight = Decimal::from_str_exact(&action.target_pct.to_string())
                    .map_err(|_| AppError::Validation("目标权重无效".into()))?
                    / Decimal::from(100);
                let value = Decimal::from_str_exact(&source.amounts.market_value)
                    .map_err(|_| AppError::Validation("资产原值无效".into()))?;
                let rate = Decimal::from_str_exact(source.fx_rate.as_deref().unwrap_or("0"))
                    .map_err(|_| AppError::Validation("折算汇率无效".into()))?;
                let difference = total
                    .checked_mul(weight)
                    .and_then(|target| value.checked_mul(rate).map(|current| target - current))
                    .ok_or_else(|| AppError::Validation("再平衡金额计算超出范围".into()))?;
                let amount = difference.abs().round_dp_with_strategy(
                    if snapshot.profile.base_currency == "JPY" {
                        0
                    } else {
                        2
                    },
                    rust_decimal::RoundingStrategy::MidpointAwayFromZero,
                );
                action.exact_amount = Some(amount.normalize().to_string());
                action.amount = amount
                    .to_f64()
                    .ok_or_else(|| AppError::Validation("再平衡金额超出范围".into()))?;
                action.direction = if difference >= Decimal::ZERO {
                    "增加"
                } else {
                    "减少"
                }
                .into();
            }
        }
        snapshot.total_value = if snapshot.valuation_status.comparable {
            derived.total_value
        } else {
            0.0
        };
        snapshot.concentration_pct = if snapshot.valuation_status.comparable {
            derived.concentration_pct
        } else {
            0.0
        };
        if !snapshot.valuation_status.decision_ready {
            snapshot.plan.rebalancing.clear();
            snapshot.plan.assumptions = format!(
                "数据质量未满足行动要求：{}。{}",
                snapshot.valuation_status.warnings.join("；"),
                snapshot.plan.assumptions
            );
        }
        snapshot.accounting = Some(accounting);
        Ok(snapshot)
    }

    pub fn save_profile(&self, profile: &FinancialProfile) -> AppResult<Snapshot> {
        let canonical = crate::models::accounting::normalize_profile(profile)?;
        let profile = &canonical;
        validate_non_negative(&[
            profile.monthly_income,
            profile.monthly_expense,
            profile.emergency_fund,
            profile.liabilities,
            profile.investable_assets,
            profile.max_drawdown_pct,
        ])?;
        if !(0..=80).contains(&profile.horizon_years) {
            return Err(AppError::Validation("投资期限应在 0—80 年之间".into()));
        }
        validate_currency(&profile.base_currency)?;
        let mut normalized = profile.clone();
        normalized.base_currency = profile.base_currency.trim().to_ascii_uppercase();
        let payload = serde_json::to_string(&normalized)?;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let before = super::daily::prepare(&transaction)?;
        let previous_base_currency = before.profile.base_currency.clone();
        transaction.execute(
            "INSERT INTO profile (id, payload, updated_at) VALUES (1, $1, $2)
             ON CONFLICT(id) DO UPDATE SET payload=excluded.payload, updated_at=excluded.updated_at",
            params![payload, Utc::now().to_rfc3339()],
        )?;
        if !previous_base_currency.eq_ignore_ascii_case(&normalized.base_currency) {
            transaction.execute(
                "UPDATE holdings SET fx_rate_to_base=NULL, fx_rate_source='',
                    fx_rate_observed_on='', updated_at=$1",
                params![Utc::now().to_rfc3339()],
            )?;
        }
        super::fx::apply_holdings(&transaction)?;
        super::daily::finish(&transaction, &before, None)?;
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn add_holding(&self, input: &HoldingInput) -> AppResult<Snapshot> {
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let before = super::daily::prepare(&transaction)?;
        let base_currency = before.profile.base_currency.clone();
        let mut input = super::fx::holding_input(&transaction, input, &base_currency)?;
        super::amounts::normalize(&mut input)?;
        super::cash::normalize_input(&mut input)?;
        super::cash::guard(&transaction, None, &input)?;
        normalize_holding_details(&mut input, false)?;
        let input = &input;
        validate_holding(input, &base_currency)?;
        let fx_rate_to_base = if input.currency.eq_ignore_ascii_case(&base_currency) {
            None
        } else {
            input.fx_rate_to_base
        };
        let (fx_rate_source, fx_rate_observed_on) = normalized_holding_fx_provenance(
            &input.currency,
            &base_currency,
            fx_rate_to_base,
            &input.fx_rate_source,
            &input.fx_rate_observed_on,
            &input.valuation_date,
        )?;
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO holdings (id, symbol, name, asset_class, market_value, cost_basis, target_pct, currency, fx_rate_to_base, valuation_date, fx_rate_source, fx_rate_observed_on, created_at, updated_at, cost_price, quantity, current_price)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13, $14, $15, $16)",
            params![id, input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.target_pct, input.currency.trim().to_ascii_uppercase(), fx_rate_to_base, input.valuation_date.trim(), fx_rate_source, fx_rate_observed_on, now, input.cost_price, input.quantity, input.current_price],
        )?;
        if input.asset_class == "现金" {
            super::cash::confirm(
                &transaction,
                &id,
                &input.currency,
                crate::models::decimal::parse(
                    input
                        .cash_amount
                        .as_deref()
                        .unwrap_or(&input.market_value.to_string()),
                )?,
                &input.valuation_date,
            )?;
            super::financial::journal(
                &transaction,
                &id,
                "balance.open",
                &Option::<Holding>::None,
                input,
                input
                    .cash_amount
                    .as_deref()
                    .unwrap_or(&input.market_value.to_string()),
                ("建立存款核对基准", None),
            )?;
        }
        super::amounts::record_input(&transaction, &id, input, false)?;
        super::fx::apply_holdings(&transaction)?;
        super::daily::finish(&transaction, &before, None)?;
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn update_holding(&self, id: &str, input: &HoldingInput) -> AppResult<Snapshot> {
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let before = super::daily::prepare(&transaction)?;
        let base_currency = before.profile.base_currency.clone();
        let mut input = super::fx::holding_input(&transaction, input, &base_currency)?;
        super::amounts::normalize(&mut input)?;
        let trade_managed = super::trades::guard_edit(&transaction, id, &mut input)?;
        super::cash::normalize_input(&mut input)?;
        super::cash::guard(&transaction, Some(id), &input)?;
        normalize_holding_details(&mut input, trade_managed)?;
        let input = &input;
        validate_holding(input, &base_currency)?;
        let fx_rate_to_base = if input.currency.eq_ignore_ascii_case(&base_currency) {
            None
        } else {
            input.fx_rate_to_base
        };
        let (fx_rate_source, fx_rate_observed_on) = normalized_holding_fx_provenance(
            &input.currency,
            &base_currency,
            fx_rate_to_base,
            &input.fx_rate_source,
            &input.fx_rate_observed_on,
            &input.valuation_date,
        )?;
        let previous = transaction
            .query_row(
                "SELECT symbol, market_value, currency, valuation_date, quantity, current_price FROM holdings WHERE id=$1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<f64>>(4)?,
                        row.get::<_, Option<f64>>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some(previous) = previous else {
            return Err(AppError::Validation("找不到要更新的资产".into()));
        };
        let affected = transaction.execute(
            "UPDATE holdings SET symbol=$2, name=$3, asset_class=$4, market_value=$5, cost_basis=$6, target_pct=$7, currency=$8, fx_rate_to_base=$9, valuation_date=$10, fx_rate_source=$11, fx_rate_observed_on=$12, updated_at=$13, cost_price=$14, quantity=$15, current_price=$16 WHERE id=$1",
            params![id, input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.target_pct, input.currency.trim().to_ascii_uppercase(), fx_rate_to_base, input.valuation_date.trim(), fx_rate_source, fx_rate_observed_on, Utc::now().to_rfc3339(), input.cost_price, input.quantity, input.current_price],
        )?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要更新的资产".into()));
        }
        let valuation_changed = previous.0 != input.symbol.trim()
            || (previous.1 - input.market_value).abs() > 0.005
            || !previous.2.eq_ignore_ascii_case(input.currency.trim())
            || previous.3 != input.valuation_date.trim()
            || previous.4 != input.quantity
            || previous.5 != input.current_price;
        if valuation_changed {
            transaction.execute("DELETE FROM holding_valuations WHERE holding_id=$1", [id])?;
        }
        if input.asset_class == "现金" {
            super::cash::confirm(
                &transaction,
                id,
                &input.currency,
                crate::models::decimal::parse(
                    input
                        .cash_amount
                        .as_deref()
                        .unwrap_or(&input.market_value.to_string()),
                )?,
                &input.valuation_date,
            )?;
            super::financial::journal(
                &transaction,
                id,
                "balance.confirm",
                &serde_json::json!({"holding":before.holdings.iter().find(|h| h.id == id),"cashBalance":before.cash_balances.iter().find(|b|b.holding_id==id)}),
                input,
                &(crate::models::decimal::parse(
                    input
                        .cash_amount
                        .as_deref()
                        .unwrap_or(&input.market_value.to_string()),
                )? - crate::models::decimal::parse(
                    &before
                        .cash_balances
                        .iter()
                        .find(|b| b.holding_id == id)
                        .map_or_else(|| previous.1.to_string(), |b| b.amount.clone()),
                )?)
                .normalize()
                .to_string(),
                ("重新核对实际存款", None),
            )?;
        }
        super::amounts::record_input(&transaction, id, input, trade_managed)?;
        super::fx::apply_holdings(&transaction)?;
        super::daily::finish(&transaction, &before, None)?;
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn apply_verified_holding_valuation(
        &self,
        id: &str,
        quantity: f64,
        quote: &SecurityPriceQuote,
    ) -> AppResult<Snapshot> {
        validate_holding_valuation_evidence(quantity, quote)?;
        if self.register_corporate_actions(id, quote)? {
            return Err(AppError::Validation(
                "存在待确认公司行动，请先核对数量与成本调整".into(),
            ));
        }
        let market_value = quantity * quote.close;
        if !market_value.is_finite() || market_value <= 0.0 || market_value > 1e15 {
            return Err(AppError::Validation("数量乘以单位价格后的市值无效".into()));
        }
        let captured_at = Utc::now().to_rfc3339();
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let before = super::daily::prepare(&transaction)?;
        let holding_currency = transaction
            .query_row("SELECT currency FROM holdings WHERE id=$1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .ok_or_else(|| AppError::Validation("找不到要估值的资产".into()))?;
        if !holding_currency.eq_ignore_ascii_case(&quote.currency) {
            return Err(AppError::Validation(format!(
                "行情以 {} 计价，但持仓币种是 {}；请先核对代码和币种",
                quote.currency, holding_currency
            )));
        }
        let holding = before
            .holdings
            .iter()
            .find(|holding| holding.id == id)
            .ok_or_else(|| AppError::Validation("找不到要估值的资产".into()))?;
        if before.holding_trades.iter().any(|t| t.holding_id == id)
            && (holding.quantity != Some(quantity) || holding.symbol != quote.symbol.trim())
        {
            return Err(AppError::Validation(
                "交易持仓的数量和证券代码不能通过估值修改".into(),
            ));
        }
        validate_detail_totals(
            holding.cost_price,
            Some(quantity),
            Some(quote.close),
            market_value,
            holding
                .cost_price
                .map_or(holding.cost_basis, |cost| cost * quantity),
        )?;
        transaction.execute(
            "UPDATE holdings
             SET symbol=$2, market_value=$3, valuation_date=$4,
                 fx_rate_to_base=CASE WHEN valuation_date=$4 THEN fx_rate_to_base ELSE NULL END,
                 fx_rate_source=CASE WHEN valuation_date=$4 THEN fx_rate_source ELSE '' END,
                 fx_rate_observed_on=CASE WHEN valuation_date=$4 THEN fx_rate_observed_on ELSE '' END,
                 updated_at=$5, quantity=$6, current_price=$7,
                 cost_basis=CASE WHEN cost_price IS NOT NULL THEN cost_price * $6 ELSE cost_basis END
             WHERE id=$1",
            params![id, quote.symbol.trim(), market_value, quote.requested_on, captured_at, quantity, quote.close],
        )?;
        transaction.execute(
            "INSERT INTO holding_valuations (
               holding_id, symbol, quantity, unit_price, market_value, currency,
               requested_on, observed_on, staleness_days, provider_code, provider_name,
               exchange_name, mic_code, instrument_type, price_basis, source_url,
               methodology_url, disclaimer, captured_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
             ON CONFLICT(holding_id) DO UPDATE SET
               symbol=excluded.symbol, quantity=excluded.quantity,
               unit_price=excluded.unit_price, market_value=excluded.market_value,
               currency=excluded.currency, requested_on=excluded.requested_on,
               observed_on=excluded.observed_on, staleness_days=excluded.staleness_days,
               provider_code=excluded.provider_code, provider_name=excluded.provider_name,
               exchange_name=excluded.exchange_name, mic_code=excluded.mic_code,
               instrument_type=excluded.instrument_type, price_basis=excluded.price_basis,
               source_url=excluded.source_url, methodology_url=excluded.methodology_url,
               disclaimer=excluded.disclaimer, captured_at=excluded.captured_at",
            params![
                id,
                quote.symbol.trim(),
                quantity,
                quote.close,
                market_value,
                quote.currency.trim().to_ascii_uppercase(),
                quote.requested_on,
                quote.observed_on,
                quote.staleness_days,
                quote.provider_code,
                quote.provider_name,
                quote.exchange,
                quote.mic_code,
                quote.instrument_type,
                quote.price_basis,
                quote.source_url,
                quote.methodology_url,
                quote.disclaimer,
                captured_at,
            ],
        )?;
        super::amounts::apply_quote(&transaction, id, quantity, quote)?;
        super::fx::apply_holdings(&transaction)?;
        super::daily::finish(&transaction, &before, None)?;
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn delete_holding(
        &self,
        id: &str,
        input: &crate::models::HoldingDeleteInput,
    ) -> AppResult<Snapshot> {
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let operation = super::financial::operation("holding.delete", id, input)?;
        if let Some(snapshot) = super::financial::replay(&transaction, &operation)? {
            return Ok(snapshot);
        }
        let before = super::daily::prepare(&transaction)?;
        let Some(holding) = before.holdings.iter().find(|h| h.id == id) else {
            return Err(AppError::Validation("找不到要删除的资产".into()));
        };
        if before.cleared_trade_holdings.iter().any(|h| h == id) {
            return Err(AppError::Validation(
                "存在隔离交易链，请先修复原始记录".into(),
            ));
        }
        let trades: Vec<_> = before
            .holding_trades
            .iter()
            .filter(|t| t.holding_id == id)
            .collect();
        if (!trades.is_empty() || !input.expected_revision.is_empty())
            && before.holding_revisions.get(id) != Some(&input.expected_revision)
        {
            return Err(AppError::Conflict(
                "持仓已更新，请刷新并核对后重新删除".into(),
            ));
        }
        let cash_delta = if !trades.is_empty() {
            super::cash::remove_holding_trades(&transaction, &holding.currency, &trades)?
                .normalize()
                .to_string()
        } else {
            before
                .cash_balances
                .iter()
                .find(|b| b.holding_id == id)
                .map(|b| {
                    crate::models::decimal::parse(&b.amount).map(|n| (-n).normalize().to_string())
                })
                .transpose()?
                .unwrap_or_else(|| "0".into())
        };
        super::financial::journal(
            &transaction,
            id,
            "holding.delete",
            &serde_json::json!({"holding":holding,"trades":trades,"accounting":before.accounting.as_ref().and_then(|a|a.holdings.get(id)),"cashBalance":before.cash_balances.iter().find(|b|b.holding_id==id),"corporateActions":before.corporate_actions.iter().filter(|a|a.holding_id==id).collect::<Vec<_>>()}),
            &Option::<Holding>::None,
            &cash_delta,
            ("用户删除持仓及关联交易", Some(&operation.0)),
        )?;
        transaction.execute("DELETE FROM holding_trades WHERE holding_id=$1", [id])?;
        transaction.execute("DELETE FROM holdings WHERE id=$1", [id])?;
        transaction.execute("DELETE FROM cash_state WHERE holding_id=$1", [id])?;
        transaction.execute("DELETE FROM corporate_actions WHERE holding_id=$1", [id])?;
        super::fx::apply_holdings(&transaction)?;
        super::daily::finish(&transaction, &before, None)?;
        let snapshot = super::financial::complete(&transaction, &operation)?;
        transaction.commit()?;
        Ok(snapshot)
    }
}

fn normalize_holding_details(input: &mut HoldingInput, trade_managed: bool) -> AppResult<()> {
    validate_non_negative(
        &input
            .cost_price
            .into_iter()
            .chain(input.quantity)
            .chain(input.current_price)
            .collect::<Vec<_>>(),
    )?;
    if input
        .cost_price
        .into_iter()
        .chain(input.quantity)
        .chain(input.current_price)
        .any(|value| value > 1e15)
    {
        return Err(AppError::Validation(
            "成本价、持仓数和当前价格超过允许范围".into(),
        ));
    }
    if input.quantity.is_some() != input.current_price.is_some() {
        return Err(AppError::Validation("持仓数和当前价格需一起填写".into()));
    }
    if input.cost_price.is_some() && input.quantity.is_none() {
        return Err(AppError::Validation(
            "填写成本价时请补充持仓数和当前价格".into(),
        ));
    }
    if let (Some(quantity), Some(price)) = (input.quantity, input.current_price) {
        input.market_value = quantity * price;
        if !trade_managed {
            if let Some(cost) = input.cost_price {
                input.cost_basis = quantity * cost;
            }
        }
    }
    validate_detail_totals(
        input.cost_price,
        input.quantity,
        input.current_price,
        input.market_value,
        input.cost_basis,
    )?;
    if input.cost_basis > 1e15 {
        return Err(AppError::Validation("持仓总成本超过允许范围".into()));
    }
    Ok(())
}

pub(super) fn validate_detail_totals(
    cost_price: Option<f64>,
    quantity: Option<f64>,
    current_price: Option<f64>,
    market_value: f64,
    cost_basis: f64,
) -> AppResult<()> {
    let values = cost_price.into_iter().chain(quantity).chain(current_price);
    if values
        .clone()
        .any(|value| !value.is_finite() || !(0.0..=1e15).contains(&value))
        || quantity.is_some() != current_price.is_some()
        || (cost_price.is_some() && quantity.is_none())
    {
        return Err(AppError::Validation("持仓数量和价格无效或不完整".into()));
    }
    if let (Some(quantity), Some(price)) = (quantity, current_price) {
        let matches = |actual: f64, expected: f64| {
            actual.is_finite()
                && expected.is_finite()
                && expected <= 1e15
                && (actual - expected).abs() <= 0.005_f64.max(expected.abs() * 1e-10)
        };
        if !matches(market_value, quantity * price)
            || cost_price.is_some_and(|cost| !matches(cost_basis, quantity * cost))
        {
            return Err(AppError::Validation("持仓总金额与数量、价格不一致".into()));
        }
    }
    Ok(())
}
