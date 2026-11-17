//! Persisted AI settings and model credential key; no user-record keys.
pub(in crate::db) enum AiSetting {
    Provider,
    BaseUrl,
    ModelName,
    Source,
    ServerModel,
    KeyBinding,
    Bridge,
    Presets,
    DreamState,
    ModelCredential,
}
impl AiSetting {
    pub(in crate::db) fn key(&self) -> &'static str {
        match self {
            Self::Provider => "model.provider",
            Self::BaseUrl => "model.base_url",
            Self::ModelName => "model.name",
            Self::Source => "model.source",
            Self::ServerModel => "model.server_name",
            Self::KeyBinding => "model.key_binding",
            Self::Bridge => "model.codex_bridge_id",
            Self::Presets => "analysis.settings",
            Self::DreamState => "memory.dream_state",
            Self::ModelCredential => "secret.model",
        }
    }
}
