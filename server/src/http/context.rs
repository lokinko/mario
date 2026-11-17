//! Request assembly and sealed, purpose-specific handler capabilities.
//! Only this adapter owns the concrete account store; route modules receive scoped extractors.
use crate::{
    application::{
        dividend_calendar::DividendCalendarService,
        model_settings::{ModelServices, ServerOpenAi},
        portfolio::{MarketServices, PortfolioService},
        watch_catalog::WatchCatalogService,
    },
    db::Database,
    ports::{
        ai_account::{AnalysisAccount, MemoryAccount, ModelSettingsStore},
        ai_records::{AnalysisArchive, AnalysisPresets},
        analysis::AnalysisExecutor,
        market::SecurityMarketProvider,
        user_account::PortfolioAccount,
        user_records::{
            EvidenceAccount, LedgerAccount, MethodologyAccount, ReviewAccount, WatchManagement,
        },
    },
    AppResult,
};
use axum::{extract::FromRequestParts, Extension};
use std::sync::Arc;

pub(crate) struct RequestContextParts {
    pub model_services: Arc<ModelServices>,
    pub db: Database,
    pub user_id: String,
    pub is_superuser: bool,
    pub watch_provider: Arc<dyn SecurityMarketProvider>,
    pub watch_catalog: Arc<WatchCatalogService>,
    pub dividend_calendar: Arc<DividendCalendarService>,
    pub market: Arc<MarketServices>,
    pub analysis_executor: Arc<dyn AnalysisExecutor>,
}

pub(crate) struct RequestContext {
    parts: RequestContextParts,
}
impl From<RequestContextParts> for RequestContext {
    fn from(parts: RequestContextParts) -> Self {
        Self { parts }
    }
}
impl RequestContext {
    pub fn lock_account(&self) -> AppResult<impl Send + '_> {
        self.parts.db.lock_account()
    }
    pub fn revision(&self) -> AppResult<String> {
        self.parts.db.revision()
    }
    pub fn replay_financial_request(
        &self,
        method: &str,
        path: &str,
        input: &serde_json::Value,
    ) -> AppResult<Option<crate::models::portfolio::Snapshot>> {
        self.parts.db.replay_financial_request(method, path, input)
    }
    #[cfg(test)]
    pub fn database(&self) -> &Database {
        &self.parts.db
    }
    #[cfg(test)]
    pub fn test_model_services(&self) -> &ModelServices {
        &self.parts.model_services
    }
    #[cfg(test)]
    pub fn test_catalog(&self) -> &Arc<WatchCatalogService> {
        &self.parts.watch_catalog
    }
}

macro_rules! scoped_context {
    ($name:ident) => {
        pub(crate) struct $name(Arc<RequestContext>);
        impl<S: Send + Sync> FromRequestParts<S> for $name {
            type Rejection = axum::extract::rejection::ExtensionRejection;
            async fn from_request_parts(
                parts: &mut axum::http::request::Parts,
                state: &S,
            ) -> Result<Self, Self::Rejection> {
                let Extension(context) =
                    Extension::<Arc<RequestContext>>::from_request_parts(parts, state).await?;
                Ok(Self(context))
            }
        }
    };
}
scoped_context!(PortfolioContext);
impl PortfolioContext {
    pub fn account(&self) -> &impl LedgerAccount {
        &self.0.parts.db
    }
    pub fn portfolio(&self) -> PortfolioService<'_, impl PortfolioAccount> {
        self.0.parts.market.for_account(&self.0.parts.db)
    }
    pub fn provider(&self) -> Arc<dyn SecurityMarketProvider> {
        self.0.parts.watch_provider.clone()
    }
    pub fn catalog(&self) -> &WatchCatalogService {
        &self.0.parts.watch_catalog
    }
}

scoped_context!(WatchContext);
impl WatchContext {
    pub fn account(&self) -> &impl WatchManagement {
        &self.0.parts.db
    }
    pub fn provider(&self) -> Arc<dyn SecurityMarketProvider> {
        self.0.parts.watch_provider.clone()
    }
    pub fn catalog(&self) -> &Arc<WatchCatalogService> {
        &self.0.parts.watch_catalog
    }
    pub fn calendar(&self) -> &DividendCalendarService {
        &self.0.parts.dividend_calendar
    }
    pub fn user_id(&self) -> &str {
        &self.0.parts.user_id
    }
    pub fn is_superuser(&self) -> bool {
        self.0.parts.is_superuser
    }
}

scoped_context!(EvidenceContext);
impl EvidenceContext {
    pub fn account(&self) -> &impl EvidenceAccount {
        &self.0.parts.db
    }
}
scoped_context!(MethodologyContext);
impl MethodologyContext {
    pub fn account(&self) -> &impl MethodologyAccount {
        &self.0.parts.db
    }
}
scoped_context!(ReviewContext);
impl ReviewContext {
    pub fn account(&self) -> &impl ReviewAccount {
        &self.0.parts.db
    }
}

scoped_context!(AnalysisContext);
impl AnalysisContext {
    pub fn account(&self) -> &impl AnalysisAccount {
        &self.0.parts.db
    }
    pub fn archive(&self) -> &impl AnalysisArchive {
        &self.0.parts.db
    }
    pub fn server(&self) -> Option<&ServerOpenAi> {
        self.0.parts.model_services.server.as_ref()
    }
    pub fn executor(&self) -> &dyn AnalysisExecutor {
        self.0.parts.analysis_executor.as_ref()
    }
}
scoped_context!(MemoryContext);
impl MemoryContext {
    pub fn account(&self) -> &impl MemoryAccount {
        &self.0.parts.db
    }
    pub fn server(&self) -> Option<&ServerOpenAi> {
        self.0.parts.model_services.server.as_ref()
    }
    pub fn executor(&self) -> &dyn AnalysisExecutor {
        self.0.parts.analysis_executor.as_ref()
    }
}
scoped_context!(ModelSettingsContext);
impl ModelSettingsContext {
    pub fn account(&self) -> &impl ModelSettingsStore {
        &self.0.parts.db
    }
    pub fn presets(&self) -> &impl AnalysisPresets {
        &self.0.parts.db
    }
    pub fn services(&self) -> &ModelServices {
        &self.0.parts.model_services
    }
}
scoped_context!(BridgeContext);
impl BridgeContext {
    pub fn account(&self) -> &impl ModelSettingsStore {
        &self.0.parts.db
    }
    pub fn services(&self) -> &ModelServices {
        &self.0.parts.model_services
    }
}

scoped_context!(BackupContext);
impl BackupContext {
    pub fn revision(&self) -> AppResult<String> {
        self.0.revision()
    }
    pub fn export(&self) -> AppResult<crate::db::SyncDataset> {
        self.0.parts.db.export_sync_data()
    }
    pub fn is_empty_for_import(&self) -> AppResult<bool> {
        self.0.parts.db.is_empty_for_import()
    }
    pub fn import(&self, data: &serde_json::Value) -> AppResult<()> {
        self.0.parts.db.import_account_data(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn requires_account<C: FromRequestParts<()>>() {
        let (mut parts, _) = axum::http::Request::new(()).into_parts();
        assert!(C::from_request_parts(&mut parts, &()).await.is_err());
    }

    #[tokio::test]
    async fn every_business_scope_requires_the_authenticated_account_context() {
        requires_account::<PortfolioContext>().await;
        requires_account::<WatchContext>().await;
        requires_account::<EvidenceContext>().await;
        requires_account::<MethodologyContext>().await;
        requires_account::<ReviewContext>().await;
        requires_account::<AnalysisContext>().await;
        requires_account::<MemoryContext>().await;
        requires_account::<ModelSettingsContext>().await;
        requires_account::<BridgeContext>().await;
        requires_account::<BackupContext>().await;
    }
}
