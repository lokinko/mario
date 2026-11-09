//! Pairing belongs to the authenticated account, never to caller-supplied model URLs.
use crate::{
    db::Database,
    models::{CodexBridgeStatus, ModelConfig, ModelProviderKind},
    ports::model::BridgeRegistry,
    AppError, AppResult,
};
const ID: &str = "model.codex_bridge_id";
pub(crate) fn id(db: &Database) -> AppResult<String> {
    db.setting(ID)?
        .ok_or_else(|| AppError::Validation("请先配对本机 Codex".into()))
}
pub(crate) fn status(db: &Database, bridges: &dyn BridgeRegistry) -> AppResult<CodexBridgeStatus> {
    Ok(bridges.status(db.setting(ID)?.as_deref()))
}
pub(crate) fn pair(db: &Database, bridges: &dyn BridgeRegistry) -> AppResult<String> {
    let pairing = bridges.create()?;
    if let Err(error) = (|| {
        if let Some(old) = db.setting(ID)? {
            bridges.revoke(&old);
        }
        db.set_setting(ID, &pairing.id)
    })() {
        bridges.revoke(&pairing.id);
        return Err(error);
    }
    Ok(pairing.code)
}
pub(crate) fn revoke(db: &Database, bridges: &dyn BridgeRegistry) -> AppResult<()> {
    if let Some(id) = db.setting(ID)? {
        bridges.revoke(&id);
    }
    db.delete_setting(ID)
}
pub(crate) fn enable(db: &Database, bridges: &dyn BridgeRegistry) -> AppResult<ModelConfig> {
    let state = status(db, bridges)?;
    if !state.online {
        return Err(AppError::Validation(
            "请先在本机启动桥接，待显示在线后再启用".into(),
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
pub(crate) fn save(
    db: &Database,
    model: &str,
    bridges: &dyn BridgeRegistry,
) -> AppResult<ModelConfig> {
    let state = status(db, bridges)?;
    if !state.online {
        return Err(AppError::Validation("本机桥接不在线，请重新连接".into()));
    }
    if !state.models.iter().any(|entry| entry.model == model) {
        return Err(AppError::Validation(
            "此模型不在本机 Codex 返回的模型列表中，请重新选择".into(),
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
            crate::models::CodexModel {
                model: "a".into(),
                display_name: "Model A".into(),
                description: String::new(),
                is_default: true,
            },
            crate::models::CodexModel {
                model: "b".into(),
                display_name: "Model B".into(),
                description: String::new(),
                is_default: false,
            },
        ];
        connection.advertise(&runner, "a", &models).unwrap();
        assert_eq!(status(&db, &REGISTRY).unwrap().models, models);
        assert_eq!(enable(&db, &REGISTRY).unwrap().model, "a");
        assert!(save(&db, "unknown", &REGISTRY).is_err());
        assert_eq!(save(&db, "b", &REGISTRY).unwrap().model, "b");
        assert_eq!(enable(&db, &REGISTRY).unwrap().model, "b");
        assert!(connection
            .advertise(&uuid::Uuid::new_v4().to_string(), "a", &models)
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
            &[crate::models::CodexModel {
                model: "fixture".into(),
                display_name: "Fixture".into(),
                description: String::new(),
                is_default: true,
            }],
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
