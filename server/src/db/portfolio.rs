use super::{
    build_snapshot, normalized_fx_provenance, params, validate_currency, validate_goal,
    validate_holding, validate_holding_valuation_evidence, validate_non_negative, AppError,
    AppResult, Database, FinancialProfile, Goal, GoalInput, Holding, HoldingInput,
    HoldingValuationEvidence, OptionalRow, SecurityPriceQuote, Snapshot, Utc, Uuid,
};

impl Database {
    pub fn snapshot(&self) -> AppResult<Snapshot> {
        let conn = self.conn()?;
        let profile = conn
            .query_row("SELECT payload FROM profile WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default();

        let mut goal_stmt = conn.prepare(
            "SELECT id, name, target_amount, current_amount, monthly_contribution, target_date, priority FROM goals ORDER BY created_at",
        )?;
        let goals = goal_stmt
            .query_map([], |row| {
                Ok(Goal {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    target_amount: row.get(2)?,
                    current_amount: row.get(3)?,
                    monthly_contribution: row.get(4)?,
                    target_date: row.get(5)?,
                    priority: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut holding_stmt = conn.prepare("SELECT id, symbol, name, asset_class, market_value, cost_basis, target_pct, currency, fx_rate_to_base, valuation_date, fx_rate_source, fx_rate_observed_on FROM holdings ORDER BY created_at")?;
        let holdings = holding_stmt
            .query_map([], |row| {
                Ok(Holding {
                    id: row.get(0)?,
                    symbol: row.get(1)?,
                    name: row.get(2)?,
                    asset_class: row.get(3)?,
                    market_value: row.get(4)?,
                    cost_basis: row.get(5)?,
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
        drop(goal_stmt);

        let updated_at = conn
            .query_row(
                "SELECT MAX(value) FROM (
                   SELECT updated_at AS value FROM profile
                   UNION ALL
                   SELECT CASE WHEN updated_at='' THEN created_at ELSE updated_at END FROM goals
                   UNION ALL
                   SELECT CASE WHEN updated_at='' THEN created_at ELSE updated_at END FROM holdings
                 )",
                [],
                |row| row.get::<_, Option<String>>(0),
            )?
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        Ok(build_snapshot(
            profile,
            goals,
            holdings,
            holding_valuations,
            updated_at,
        ))
    }

    pub fn save_profile(&self, profile: &FinancialProfile) -> AppResult<Snapshot> {
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
        let previous_base_currency = self.snapshot()?.profile.base_currency;
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO profile (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET payload=excluded.payload, updated_at=excluded.updated_at",
            params![payload, Utc::now().to_rfc3339()],
        )?;
        if !previous_base_currency.eq_ignore_ascii_case(&normalized.base_currency) {
            transaction.execute(
                "UPDATE holdings SET fx_rate_to_base=NULL, fx_rate_source='',
                    fx_rate_observed_on='', updated_at=?1",
                params![Utc::now().to_rfc3339()],
            )?;
        }
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn add_holding(&self, input: &HoldingInput) -> AppResult<Snapshot> {
        let base_currency = self.snapshot()?.profile.base_currency;
        validate_holding(input, &base_currency)?;
        let fx_rate_to_base = if input.currency.eq_ignore_ascii_case(&base_currency) {
            None
        } else {
            input.fx_rate_to_base
        };
        let (fx_rate_source, fx_rate_observed_on) = normalized_fx_provenance(
            &input.currency,
            &base_currency,
            fx_rate_to_base,
            &input.fx_rate_source,
            &input.fx_rate_observed_on,
            &input.valuation_date,
        )?;
        let now = Utc::now().to_rfc3339();
        self.conn()?.execute(
            "INSERT INTO holdings (id, symbol, name, asset_class, market_value, cost_basis, target_pct, currency, fx_rate_to_base, valuation_date, fx_rate_source, fx_rate_observed_on, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
            params![Uuid::new_v4().to_string(), input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.target_pct, input.currency.trim().to_ascii_uppercase(), fx_rate_to_base, input.valuation_date.trim(), fx_rate_source, fx_rate_observed_on, now],
        )?;
        self.snapshot()
    }

    pub fn update_holding(&self, id: &str, input: &HoldingInput) -> AppResult<Snapshot> {
        let base_currency = self.snapshot()?.profile.base_currency;
        validate_holding(input, &base_currency)?;
        let fx_rate_to_base = if input.currency.eq_ignore_ascii_case(&base_currency) {
            None
        } else {
            input.fx_rate_to_base
        };
        let (fx_rate_source, fx_rate_observed_on) = normalized_fx_provenance(
            &input.currency,
            &base_currency,
            fx_rate_to_base,
            &input.fx_rate_source,
            &input.fx_rate_observed_on,
            &input.valuation_date,
        )?;
        let mut conn = self.conn()?;
        let previous = conn
            .query_row(
                "SELECT symbol, market_value, currency, valuation_date FROM holdings WHERE id=?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some(previous) = previous else {
            return Err(AppError::Validation("找不到要更新的资产".into()));
        };
        let transaction = conn.transaction()?;
        let affected = transaction.execute(
            "UPDATE holdings SET symbol=?2, name=?3, asset_class=?4, market_value=?5, cost_basis=?6, target_pct=?7, currency=?8, fx_rate_to_base=?9, valuation_date=?10, fx_rate_source=?11, fx_rate_observed_on=?12, updated_at=?13 WHERE id=?1",
            params![id, input.symbol.trim(), input.name.trim(), input.asset_class, input.market_value, input.cost_basis, input.target_pct, input.currency.trim().to_ascii_uppercase(), fx_rate_to_base, input.valuation_date.trim(), fx_rate_source, fx_rate_observed_on, Utc::now().to_rfc3339()],
        )?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要更新的资产".into()));
        }
        let valuation_changed = previous.0 != input.symbol.trim()
            || (previous.1 - input.market_value).abs() > 0.005
            || !previous.2.eq_ignore_ascii_case(input.currency.trim())
            || previous.3 != input.valuation_date.trim();
        if valuation_changed {
            transaction.execute("DELETE FROM holding_valuations WHERE holding_id=?1", [id])?;
        }
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
        let market_value = quantity * quote.close;
        if !market_value.is_finite() || market_value <= 0.0 || market_value > 1e15 {
            return Err(AppError::Validation("数量乘以单位价格后的市值无效".into()));
        }
        let captured_at = Utc::now().to_rfc3339();
        let mut conn = self.conn()?;
        let holding_currency = conn
            .query_row("SELECT currency FROM holdings WHERE id=?1", [id], |row| {
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
        let transaction = conn.transaction()?;
        transaction.execute(
            "UPDATE holdings
             SET symbol=?2, market_value=?3, valuation_date=?4,
                 fx_rate_to_base=CASE WHEN valuation_date=?4 THEN fx_rate_to_base ELSE NULL END,
                 fx_rate_source=CASE WHEN valuation_date=?4 THEN fx_rate_source ELSE '' END,
                 fx_rate_observed_on=CASE WHEN valuation_date=?4 THEN fx_rate_observed_on ELSE '' END,
                 updated_at=?5
             WHERE id=?1",
            params![id, quote.symbol.trim(), market_value, quote.requested_on, captured_at],
        )?;
        transaction.execute(
            "INSERT INTO holding_valuations (
               holding_id, symbol, quantity, unit_price, market_value, currency,
               requested_on, observed_on, staleness_days, provider_code, provider_name,
               exchange_name, mic_code, instrument_type, price_basis, source_url,
               methodology_url, disclaimer, captured_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
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
        transaction.commit()?;
        drop(conn);
        self.snapshot()
    }

    pub fn delete_holding(&self, id: &str) -> AppResult<Snapshot> {
        let affected = self
            .conn()?
            .execute("DELETE FROM holdings WHERE id=?1", [id])?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要删除的资产".into()));
        }
        self.snapshot()
    }

    pub fn add_goal(&self, input: &GoalInput) -> AppResult<Snapshot> {
        validate_goal(input)?;
        let now = Utc::now().to_rfc3339();
        self.conn()?.execute(
            "INSERT INTO goals (id, name, target_amount, current_amount, monthly_contribution, target_date, priority, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![Uuid::new_v4().to_string(), input.name.trim(), input.target_amount, input.current_amount, input.monthly_contribution, input.target_date, input.priority, now],
        )?;
        self.snapshot()
    }

    pub fn update_goal(&self, id: &str, input: &GoalInput) -> AppResult<Snapshot> {
        validate_goal(input)?;
        let affected = self.conn()?.execute(
            "UPDATE goals SET name=?2, target_amount=?3, current_amount=?4, monthly_contribution=?5, target_date=?6, priority=?7, updated_at=?8 WHERE id=?1",
            params![id, input.name.trim(), input.target_amount, input.current_amount, input.monthly_contribution, input.target_date, input.priority, Utc::now().to_rfc3339()],
        )?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要更新的目标".into()));
        }
        self.snapshot()
    }

    pub fn delete_goal(&self, id: &str) -> AppResult<Snapshot> {
        let affected = self
            .conn()?
            .execute("DELETE FROM goals WHERE id=?1", [id])?;
        if affected == 0 {
            return Err(AppError::Validation("找不到要删除的目标".into()));
        }
        self.snapshot()
    }
}
