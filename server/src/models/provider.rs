use serde::{Deserialize, Serialize};

use crate::{AppError, AppResult};

/// Model endpoint family. The kebab-case form is the stored setting value and wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelProviderKind {
    OpenaiResponses,
    OpenaiChat,
    Anthropic,
    /// Codex running on the user's computer, reached through the paired bridge.
    CodexBridge,
}

impl ModelProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenaiResponses => "openai-responses",
            Self::OpenaiChat => "openai-chat",
            Self::Anthropic => "anthropic",
            Self::CodexBridge => "codex-bridge",
        }
    }

    /// Native providers call a remote HTTP model API with a stored API key.
    pub const fn is_native(self) -> bool {
        matches!(
            self,
            Self::OpenaiResponses | Self::OpenaiChat | Self::Anthropic
        )
    }
}

impl std::fmt::Display for ModelProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ModelProviderKind {
    type Err = AppError;

    fn from_str(value: &str) -> AppResult<Self> {
        Ok(match value {
            "openai-responses" => Self::OpenaiResponses,
            "openai-chat" => Self::OpenaiChat,
            "anthropic" => Self::Anthropic,
            "codex-bridge" => Self::CodexBridge,
            _ => {
                return Err(AppError::Validation(
                    "请选择 OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 或本机 Codex 桥接"
                        .into(),
                ))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ModelProviderKind;

    #[test]
    fn stored_and_wire_names_round_trip() {
        for kind in [
            ModelProviderKind::OpenaiResponses,
            ModelProviderKind::OpenaiChat,
            ModelProviderKind::Anthropic,
            ModelProviderKind::CodexBridge,
        ] {
            assert_eq!(kind.as_str().parse::<ModelProviderKind>().unwrap(), kind);
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
        }
        assert!("openai-compatible".parse::<ModelProviderKind>().is_err());
        assert!("codex".parse::<ModelProviderKind>().is_err());
    }
}
