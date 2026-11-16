pub(crate) mod bridge;
pub(crate) mod chatgpt;
mod codex;
mod council;
mod dreaming;
pub(crate) mod management;
pub(crate) mod model_catalog;
pub use codex::CodexProvider;
mod native_provider;
mod orchestrator;
mod provider;
#[cfg(test)]
mod qa_eval;
pub(crate) mod streaming;
mod structured_output;
mod workflow;

pub use dreaming::dream;
pub use native_provider::NativeModelProvider;
#[cfg(test)]
pub(crate) use orchestrator::tests::default_reply as council_fixture_reply;
pub use orchestrator::InvestmentOrchestrator;
pub use provider::{ChatMessage, ModelProvider};
#[cfg(test)]
pub use provider::{ModelCompletion, ModelUsage};
