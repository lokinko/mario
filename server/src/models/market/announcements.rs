//! Public statutory corporate announcements owned by the market domain.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnouncementImportance {
    High,
    Medium,
    Normal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialAnnouncement {
    pub id: String,
    pub symbol: String,
    pub stock_name: String,
    pub title: String,
    pub category: String,
    pub published_on: String,
    pub published_at: String,
    pub source_name: String,
    pub source_url: String,
    pub importance: AnnouncementImportance,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementSnapshot {
    #[serde(default)]
    pub announcements: Vec<OfficialAnnouncement>,
    #[serde(default)]
    pub attempted_at: Option<String>,
    #[serde(default)]
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub refresh_failed: bool,
}

/// Deterministic category and importance classifier for statutory A-share announcements.
pub(crate) fn classify_announcement(title: &str) -> (&'static str, AnnouncementImportance) {
    let trimmed = title.trim();
    if [
        "年度报告",
        "半年度报告",
        "第一季度报告",
        "第三季度报告",
        "业绩预告",
        "业绩快报",
        "利润分配",
        "权益分派",
        "分红派息",
        "中期分红",
        "特别分红",
        "回购股份",
        "增持",
        "减持",
        "重大资产重组",
        "要约收购",
        "停牌",
        "复牌",
        "风险提示",
        "立案",
        "处罚",
        "退市",
        "业绩说明会",
    ]
    .iter()
    .any(|keyword| trimmed.contains(keyword))
    {
        let category = if ["利润分配", "权益分派", "分红派息", "中期分红", "特别分红"]
            .iter()
            .any(|k| trimmed.contains(k))
        {
            "分红派息"
        } else if [
            "年度报告",
            "半年度报告",
            "第一季度报告",
            "第三季度报告",
            "业绩预告",
            "业绩快报",
            "业绩说明会",
        ]
        .iter()
        .any(|k| trimmed.contains(k))
        {
            "定期报告与业绩"
        } else if ["回购股份", "增持", "减持"]
            .iter()
            .any(|k| trimmed.contains(k))
        {
            "股份变动与回购"
        } else {
            "重大事项"
        };
        return (category, AnnouncementImportance::High);
    }

    if [
        "董事会",
        "股东大会",
        "监事会",
        "对外投资",
        "担保",
        "关联交易",
        "可转债",
        "股权激励",
        "高管",
        "董事长",
        "总经理",
        "中标",
        "合同",
        "经营数据",
        "产销快报",
        "投资者关系",
    ]
    .iter()
    .any(|keyword| trimmed.contains(keyword))
    {
        return ("公司治理与经营", AnnouncementImportance::Medium);
    }

    ("法定公告", AnnouncementImportance::Normal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_key_statutory_announcements() {
        assert_eq!(
            classify_announcement("关于2026年半年度利润分配方案的公告"),
            ("分红派息", AnnouncementImportance::High)
        );
        assert_eq!(
            classify_announcement("2026年第三季度报告"),
            ("定期报告与业绩", AnnouncementImportance::High)
        );
        assert_eq!(
            classify_announcement("关于以集中竞价交易方式回购股份的进展公告"),
            ("股份变动与回购", AnnouncementImportance::High)
        );
        assert_eq!(
            classify_announcement("第八届董事会第十二次会议决议公告"),
            ("公司治理与经营", AnnouncementImportance::Medium)
        );
        assert_eq!(
            classify_announcement("关于公司变更办公地址的公告"),
            ("法定公告", AnnouncementImportance::Normal)
        );
    }
}
