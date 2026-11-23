//! Pairing belongs to the authenticated account, never to caller-supplied model URLs.
#[cfg(test)]
use crate::db::Database;
use crate::{
    models::{
        connection::{CodexBridgeStatus, ModelConfig},
        provider::ModelProviderKind,
    },
    ports::{
        ai_account::{ModelConfiguration, ModelSettingsStore},
        model::BridgeRegistry,
    },
    AppError, AppResult,
};
pub(crate) fn id(db: &impl ModelConfiguration) -> AppResult<String> {
    db.bridge_id()?
        .ok_or_else(|| AppError::Validation("请先配对本机 ChatGPT 桥接".into()))
}
pub(crate) fn status(
    db: &impl ModelConfiguration,
    bridges: &dyn BridgeRegistry,
) -> AppResult<CodexBridgeStatus> {
    Ok(bridges.status(db.bridge_id()?.as_deref()))
}
pub(crate) fn pair(
    db: &impl ModelSettingsStore,
    bridges: &dyn BridgeRegistry,
) -> AppResult<String> {
    let pairing = bridges.create()?;
    if let Err(error) = (|| {
        if let Some(old) = db.bridge_id()? {
            bridges.revoke(&old);
        }
        db.set_bridge_id(&pairing.id)
    })() {
        bridges.revoke(&pairing.id);
        return Err(error);
    }
    Ok(pairing.code)
}
pub(crate) fn revoke(db: &impl ModelSettingsStore, bridges: &dyn BridgeRegistry) -> AppResult<()> {
    if let Some(id) = db.bridge_id()? {
        bridges.revoke(&id);
    }
    db.clear_bridge_id()
}
pub(crate) fn enable(
    db: &impl ModelSettingsStore,
    bridges: &dyn BridgeRegistry,
) -> AppResult<ModelConfig> {
    let state = status(db, bridges)?;
    if !state.online {
        return Err(AppError::Validation(
            "请先在本机启动桥接，待显示在线后再启用".into(),
        ));
    }
    if state.auth_required {
        return Err(AppError::Validation(
            "请先在设置中点击“跳转 ChatGPT OAuth 登录”完成本机授权".into(),
        ));
    }
    let previous = db.model_config()?;
    let selected = if previous.provider == ModelProviderKind::CodexBridge
        && state
            .models
            .iter()
            .any(|entry| entry.model == previous.model)
    {
        previous.model.as_str()
    } else {
        state.model.as_deref().unwrap_or_default()
    };
    save(db, selected, bridges)
}
pub(crate) async fn start_oauth(
    db: &impl ModelConfiguration,
    bridges: &dyn BridgeRegistry,
    new_account: bool,
) -> AppResult<String> {
    let bridge_id = id(db)?;
    bridges.start_oauth(&bridge_id, new_account).await
}
pub(crate) fn save(
    db: &impl ModelSettingsStore,
    model: &str,
    bridges: &dyn BridgeRegistry,
) -> AppResult<ModelConfig> {
    let state = status(db, bridges)?;
    if !state.online {
        return Err(AppError::Validation("本机桥接不在线，请重新连接".into()));
    }
    if !state.models.iter().any(|entry| entry.model == model) {
        return Err(AppError::Validation(
            "此模型不在本机 ChatGPT 桥接返回的模型列表中，请重新选择".into(),
        ));
    }
    db.save_model_configuration(
        ModelProviderKind::CodexBridge,
        "codex://bridge",
        model,
        None,
    )?;
    db.model_config()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{bridge, management::BridgeRegistryAdapter};
    const REGISTRY: BridgeRegistryAdapter = BridgeRegistryAdapter;
    #[test]
    fn catalog_selection_is_validated_preserved_and_account_scoped() {
        let db = Database::test();
        let token = pair(&db, &REGISTRY).unwrap();
        let connection = bridge::authenticate(&token).unwrap();
        let runner = uuid::Uuid::new_v4().to_string();
        let models = vec![
            crate::models::connection::CodexModel {
                model: "a".into(),
                display_name: "Model A".into(),
                description: String::new(),
                is_default: true,
            },
            crate::models::connection::CodexModel {
                model: "b".into(),
                display_name: "Model B".into(),
                description: String::new(),
                is_default: false,
            },
        ];
        connection
            .advertise(&runner, "a", &models, None, None)
            .unwrap();
        assert_eq!(status(&db, &REGISTRY).unwrap().models, models);
        assert_eq!(enable(&db, &REGISTRY).unwrap().model, "a");
        assert!(save(&db, "unknown", &REGISTRY).is_err());
        assert_eq!(save(&db, "b", &REGISTRY).unwrap().model, "b");
        assert_eq!(enable(&db, &REGISTRY).unwrap().model, "b");
        assert!(connection
            .advertise(&uuid::Uuid::new_v4().to_string(), "a", &models, None, None)
            .is_err());
        assert_eq!(status(&db, &REGISTRY).unwrap().models, models);
        revoke(&db, &REGISTRY).unwrap();
        assert!(status(&db, &REGISTRY).unwrap().models.is_empty());
        assert_eq!(db.model_config().unwrap().model, "b");
    }
    #[tokio::test]
    async fn bridge_account_selection_cannot_be_redirected_by_a_client_model_url() {
        let a = Database::test();
        let b = Database::test();
        let token_a = pair(&a, &REGISTRY).unwrap();
        let token_b = pair(&b, &REGISTRY).unwrap();
        assert_ne!(id(&a).unwrap(), id(&b).unwrap());
        assert!(!status(&a, &REGISTRY).unwrap().online);
        assert!(enable(&b, &REGISTRY).is_err());
        let ca = bridge::authenticate(&token_a).unwrap();
        let runner = uuid::Uuid::new_v4().to_string();
        ca.advertise(
            &runner,
            "fixture",
            &[crate::models::connection::CodexModel {
                model: "fixture".into(),
                display_name: "Fixture".into(),
                description: String::new(),
                is_default: true,
            }],
            None,
            None,
        )
        .unwrap();
        let poll = tokio::spawn(async move { ca.poll(&runner, "fixture").await });
        tokio::task::yield_now().await;
        assert!(status(&a, &REGISTRY).unwrap().online);
        assert!(!status(&b, &REGISTRY).unwrap().online);
        // Saving a supplied endpoint cannot borrow another account's connection.
        assert!(save(&b, "fixture", &REGISTRY).is_err());
        let configured = enable(&a, &REGISTRY).unwrap();
        assert_eq!(configured.base_url, "codex://bridge");
        let backup = serde_json::to_string(&a.export_sync_data().unwrap()).unwrap();
        assert!(!backup.contains(&id(&a).unwrap()));
        assert!(!backup.contains(&token_a));
        revoke(&b, &REGISTRY).unwrap();
        assert!(bridge::authenticate(&token_a).is_ok());
        assert!(bridge::authenticate(&token_b).is_err());
        pair(&a, &REGISTRY).unwrap();
        assert!(bridge::authenticate(&token_a).is_err());
        revoke(&a, &REGISTRY).unwrap();
        assert!(id(&a).is_err());
        assert!(!a.model_config().unwrap().has_api_key);
        poll.abort();
    }
}
