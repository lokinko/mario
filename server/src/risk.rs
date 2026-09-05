use crate::models::{FinancialProfile, Holding, RiskFinding};

pub fn analyze(profile: &FinancialProfile, holdings: &[Holding]) -> Vec<RiskFinding> {
    let mut findings = Vec::new();
    let total: f64 = holdings.iter().map(|h| h.market_value).sum();
    let emergency_months = if profile.monthly_expense > 0.0 {
        profile.emergency_fund / profile.monthly_expense
    } else {
        0.0
    };

    if profile.monthly_expense > 0.0 && emergency_months < 3.0 {
        findings.push(RiskFinding {
            level: "high".into(),
            title: "应急资金不足".into(),
            detail: format!(
                "当前只能覆盖约 {:.1} 个月支出，市场下跌时可能被迫卖出。",
                emergency_months
            ),
            action: "优先建立至少 3—6 个月刚性支出的安全垫。".into(),
        });
    } else if emergency_months >= 6.0 {
        findings.push(RiskFinding {
            level: "low".into(),
            title: "应急资金较充足".into(),
            detail: format!("当前可覆盖约 {:.1} 个月支出。", emergency_months),
            action: "收入或重大支出变化后重新评估。".into(),
        });
    }

    if total > 0.0 {
        if let Some(largest) = holdings
            .iter()
            .max_by(|a, b| a.market_value.total_cmp(&b.market_value))
        {
            let pct = largest.market_value / total * 100.0;
            if pct > 40.0 && largest.asset_class != "现金" {
                findings.push(RiskFinding {
                    level: if pct > 65.0 {
                        "high".into()
                    } else {
                        "medium".into()
                    },
                    title: "组合存在集中暴露".into(),
                    detail: format!(
                        "“{}”占组合 {:.1}%，单一判断会显著影响整体结果。",
                        largest.name, pct
                    ),
                    action: "检查该仓位的永久损失情景，以及其他持仓是否具有相同风险来源。".into(),
                });
            }
        }
    }

    if profile.liabilities > profile.monthly_income * 12.0 && profile.monthly_income > 0.0 {
        findings.push(RiskFinding {
            level: "medium".into(),
            title: "负债可能压缩风险容量".into(),
            detail: "负债余额超过一年收入，真实风险承受能力可能低于主观风险偏好。".into(),
            action: "结合利率、还款期限和收入稳定性，优先评估偿债方案。".into(),
        });
    }

    if profile.investable_assets > 0.0 {
        let difference_pct =
            (total - profile.investable_assets).abs() / profile.investable_assets * 100.0;
        if difference_pct > 10.0 {
            findings.push(RiskFinding {
                level: "medium".into(),
                title: "资产档案尚未对齐".into(),
                detail: format!(
                    "持仓市值与填写的可投资资产相差 {:.1}%，当前组合分析可能不完整。",
                    difference_pct
                ),
                action: "补齐遗漏资产，或更新财务档案中的可投资资产总额。".into(),
            });
        }
    }

    if profile.horizon_years <= 2
        && holdings
            .iter()
            .any(|h| matches!(h.asset_class.as_str(), "股票" | "基金"))
    {
        findings.push(RiskFinding {
            level: "medium".into(),
            title: "期限与波动资产可能不匹配".into(),
            detail: "投资期限不超过两年，但组合中包含股票或基金。".into(),
            action: "确认短期目标资金不会因市场波动而被迫变现。".into(),
        });
    }

    if findings.is_empty() {
        findings.push(RiskFinding {
            level: "low".into(),
            title: "未发现明显的规则型风险".into(),
            detail: "这不表示组合没有风险，只表示当前输入未触发基础检查。".into(),
            action: "继续完善目标、持仓和决策记录。".into(),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::FinancialProfile;

    #[test]
    fn detects_thin_emergency_fund() {
        let profile = FinancialProfile {
            monthly_expense: 10_000.0,
            emergency_fund: 15_000.0,
            ..Default::default()
        };
        let result = analyze(&profile, &[]);
        assert!(result.iter().any(|x| x.title.contains("应急资金不足")));
    }
}
