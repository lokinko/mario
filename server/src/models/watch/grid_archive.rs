//! Read-only migration evidence for pre-share-unit grid plans. No execution path.
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridAllocationArchive {
    pub buy_basis: String,
    pub sell_basis: String,
    pub levels: Vec<AllocationLevel>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AllocationLevel {
    pub yield_bps: u16,
    pub buy_bps: u16,
    pub sell_bps: u16,
}
impl GridAllocationArchive {
    pub fn validate(&self) -> AppResult<()> {
        let mut seen = std::collections::BTreeSet::new();
        if !["budget", "remainingCash"].contains(&self.buy_basis.as_str())
            || !["totalBought", "remainingShares"].contains(&self.sell_basis.as_str())
            || self.levels.is_empty()
            || self.levels.len() > 21
            || self.levels.iter().any(|l| {
                !(400..=600).contains(&l.yield_bps)
                    || l.yield_bps % 10 != 0
                    || !seen.insert(l.yield_bps)
                    || l.buy_bps > 10_000
                    || l.sell_bps > 10_000
            })
        {
            return Err(AppError::Validation("旧网格历史参考格式无效".into()));
        }
        Ok(())
    }
}
