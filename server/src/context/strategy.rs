//! Explicit investment decision rules and domain strategy playbooks injected into frozen context.
//! These define the analytical framework and decision discipline; they are never financial facts.
use serde_json::{json, Value};

use super::discussion::InvestmentDomain;

/// Core investment discipline applied across all specialist modules and final synthesis.
pub const CORE_DECISION_RULES: &[&str] = &[
    "先给清晰动作立场（如买入/分批加仓/持有观察/逢高减仓/回避），再用核心定价逻辑支撑；严禁模棱两可或堆砌免责套话。",
    "禁止“作业外包”：不得把“请你自己查带时间戳报价、翻财报算多期盈利与现金流、逐条核对公告”写成核心建议；建议必须落到具体投资决策或可执行的触发条件。",
    "禁止成段复读风控常识或提示词条文（如反复解释“浮亏不等于低估”“参考价不是实时成交价”“覆盖未知不等于缺口”）；必要口径提示最多半句带过。",
    "浮亏补仓须同时满足三条硬规则：①核心基本面与盈利中枢未恶化；②当前估值相对周期中枢/保守盈利有明确安全边际；③补仓后不突破单票及同周期行业仓位上限。低于个人持仓成本本身绝不构成买点。",
    "卖出与减仓仅由三类原因驱动：核心逻辑证伪（果断止损退出）、估值显著透支赔率（分批兑现）、单一风险敞口超配（组合再平衡）；彻底剥离沉没成本，不因“等回本”拒绝处置。",
    "网页检索线索（webLeads）虽不可写入 facts.evidenceIds 或冒充一手公告原文，但分析中应充分利用其中已查明的经营、估值与行业背景（注明“据网页线索，待公告复核”），不得假装一无所知。",
    "用户保存的自定义投资方法论作为最高个人偏好与本策略基准融合执行；如存在风格偏好冲突，以用户明确保存的方法论为准，但不得突破事实真实性与防编造底线。",
];

/// Detect cyclical/commodity stocks or discussions where peak-earnings P/E traps are critical.
pub fn is_cyclical_context(question: &str, facts: &Value, companies: &[String]) -> bool {
    const CYCLICAL_KEYWORDS: &[&str] = &[
        "周期",
        "铝",
        "云铝",
        "铜",
        "锌",
        "锂",
        "煤炭",
        "神华",
        "原油",
        "石油",
        "海油",
        "石化",
        "钢铁",
        "化工",
        "航运",
        "海运",
        "有色",
        "稀土",
        "黄金",
        "矿业",
        "电解铝",
        "氧化铝",
        "水泥",
        "建材",
        "猪周期",
        "养殖",
    ];
    if CYCLICAL_KEYWORDS.iter().any(|kw| question.contains(kw)) {
        return true;
    }
    if let Some(holdings) = facts["holdings"].as_object() {
        for row in holdings.values() {
            let name = row["name"].as_str().unwrap_or_default();
            let symbol = row["symbol"].as_str().unwrap_or_default();
            let relevant = companies.is_empty()
                || companies.iter().any(|c| c == symbol)
                || (!name.is_empty() && question.contains(name));
            if relevant && CYCLICAL_KEYWORDS.iter().any(|kw| name.contains(kw)) {
                return true;
            }
        }
    }
    false
}

pub fn domain_playbook(domain: InvestmentDomain) -> (&'static str, Vec<&'static str>) {
    match domain {
        InvestmentDomain::DividendDiscussion => (
            "高股息与红利投资策略",
            vec![
                "质量过滤：优先核查常规派息比例与历史连续性，剥离一次性特别分红；非金融看“经营现金流 - 维持性资本开支”对实付股息的覆盖，金融业看资本充足率、拨备与拨备前利润。",
                "估值与买点纪律：买点锚定在“可持续常规股息率显著高于无风险利率与历史中枢 + 业务需求未出现结构性坍塌”；严防盈利下滑导致的“高股息价值陷阱”。",
                "技术信号边界：BOLL/均线等技术位置仅作分批择时的辅助触发器，不能单独证明低估或替代分红覆盖校验。",
            ],
        ),
        InvestmentDomain::EquityValuation => (
            "个股基本面与估值定价策略",
            vec![
                "盈利质量与定价锚：区分结构性成长、稳定现金牛与强周期波动，选用匹配估值锚（成长看增速与FCF质量、稳定现金牛看FCF收益率、重资产/周期看PB与中枢盈利）。",
                "安全边际规则：倒推当前估值隐含的营收、利润率或商品价格假设；只有在保守/中枢情景下仍具合理赔率时才认定具备买点。",
                "持仓应对：已有持仓且处于中性估值区间时，优先明确“继续持有的底线”与“向上减仓/向下加仓”的双边触发条件，不给空泛等待。",
            ],
        ),
        InvestmentDomain::ExitOrRebalance => (
            "卖出、止损与换仓执行策略",
            vec![
                "前瞻赔率决策：把当前持仓视作等额现金重新评估——如果今天手握现金不会在现价买入该仓位，就应评估减仓或换仓，不以“回本”为持有理由。",
                "资金去向与分母口径：卖款留在组合内再平衡按原总值测算；用于提款、消费或还债须按转出后的新组合总值与现金缺口联动核算，并计入交易与税费摩擦。",
                "分批与纪律：优先给可复核的分批触发条件（估值/基本面/仓位阈值），避免全凭情绪一次性清仓或死扛。",
            ],
        ),
        InvestmentDomain::PortfolioAllocation => (
            "资产配置与组合风控策略",
            vec![
                "安全垫优先：先对齐授权的应急金、负债与刚性用款期限，确保短期必要现金完全脱离权益波动，再分配风险预算。",
                "集中度与尾部防线：审视单一标的、单一高相关行业与单一币种敞口；评估-30%级别权益回撤下组合是否仍不触发被迫卖出。",
                "增量与再平衡：优先用每月新增结余修补低配资产或现金缓冲，减少不必要的存量交易摩擦。",
            ],
        ),
        InvestmentDomain::CashflowAndDebt => (
            "现金流与负债配置策略",
            vec![
                "流动性分层：短期刚性用款与应急备用金放在高流动性低波动工具中，不与长期投资资金混用。",
                "利差与确定性权衡：将确定性负债利率成本与风险资产不确定预期回报做风险调整后比较，同时衡量提前还贷失去流动性的期权成本。",
                "净结余口径：已计入月支出的正常还款不重复扣减，只额外预留尚未计入预算的大额现金需求。",
            ],
        ),
        InvestmentDomain::OptionsOrHedging => (
            "期权与对冲风险控制策略",
            vec![
                "尾部风险硬底线：严禁无保护裸卖无限亏损头寸；现金担保卖沽（CSP）须按愿意接盘的合理估值定行权价并预留足额履约现金。",
                "收益成本比：备兑看涨（Covered Call）须衡量权利金年化补偿 vs 标的反弹踏空成本与除息提前行权风险。",
                "简单替代检验：任何对冲策略须与“直接减仓”或“增持现金”对比净成本与复杂度，更简单方案更优时直接推荐简单方案。",
            ],
        ),
        InvestmentDomain::MacroOrSector => (
            "宏观与行业周期传导策略",
            vec![
                "具体传导落地：只沿可验证的传导链条（利率→折现率、商品价格→单位毛利、汇率→资产估值与汇兑）分析，不写泛泛宏大叙事。",
                "双向情景与先行指标：同时明确基准情景与反向情景对当前持仓的实质敏感度，指出 1–2 个决定方向切换的先行跟踪指标。",
            ],
        ),
        InvestmentDomain::GeneralFinance => (
            "轻量核查与直答规则",
            vec![
                "直击问题本身：核对授权数据、口径或金融概念，直接给出准确结论与边界，不强行扩写无关的配置或交易方案。",
            ],
        ),
    }
}

pub fn build_strategy_context(
    domain: InvestmentDomain,
    question: &str,
    facts: &Value,
    companies: &[String],
) -> Value {
    let (playbook_title, mut rules) = domain_playbook(domain);
    let cyclical = is_cyclical_context(question, facts, companies);
    if cyclical && domain != InvestmentDomain::GeneralFinance {
        rules.push("【周期/资源股专项规则】严禁拿盈利高点的低PE当低估！必须从“供给/产能天花板 + 公司在行业成本曲线的分位（如水电/资源自给率） + 周期中枢盈利/PB重置成本 + 商品价格回落时的盈利底线”四个维度定买卖点；景气高点忌追高，低谷或中枢下沿且具备成本优势时才具高赔率。");
    }
    json!({
        "playbookTitle": playbook_title,
        "cyclicalOverlay": cyclical,
        "coreRules": CORE_DECISION_RULES,
        "strategyRules": rules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyclical_overlay_triggers_for_commodity_stocks_and_holdings() {
        let facts = json!({
            "holdings": {
                "h1": {"name": "云铝股份", "symbol": "000807.SZ"}
            }
        });
        assert!(is_cyclical_context("云铝股份现在是买点吗？", &facts, &[]));
        let strategy = build_strategy_context(
            InvestmentDomain::EquityValuation,
            "云铝股份现在是买点吗？",
            &facts,
            &["000807.SZ".into()],
        );
        assert_eq!(strategy["cyclicalOverlay"], true);
        assert!(strategy["strategyRules"]
            .to_string()
            .contains("周期/资源股专项规则"));
        assert!(strategy["coreRules"].to_string().contains("禁止“作业外包”"));
    }

    #[test]
    fn non_cyclical_query_omits_cyclical_overlay() {
        let facts = json!({
            "holdings": {
                "h1": {"name": "指数基金", "symbol": "510300.SS"}
            }
        });
        let strategy = build_strategy_context(
            InvestmentDomain::PortfolioAllocation,
            "我的组合集中度是否太高？",
            &facts,
            &[],
        );
        assert_eq!(strategy["cyclicalOverlay"], false);
    }
}
