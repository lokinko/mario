//! Pairing belongs to the authenticated account, never to caller-supplied model URLs.
use crate::{ai::bridge, db::Database, models::ModelConfig, AppError, AppResult};
const ID: &str = "model.codex_bridge_id";
pub(crate) fn id(db: &Database) -> AppResult<String> {
    db.setting(ID)?
        .ok_or_else(|| AppError::Validation("请先配对本机 Codex".into()))
}
pub(crate) fn status(db: &Database) -> AppResult<bridge::Status> {
    Ok(db
        .setting(ID)?
        .map(|id| bridge::status(&id))
        .unwrap_or_default())
}
pub(crate) fn pair(db: &Database) -> AppResult<String> {
    let (new_id, token) = bridge::create()?;
    if let Err(error) = (|| {
        if let Some(old) = db.setting(ID)? {
            bridge::revoke(&old);
        }
        db.set_setting(ID, &new_id)
    })() {
        bridge::revoke(&new_id);
        return Err(error);
    }
    Ok(token)
}
pub(crate) fn revoke(db: &Database) -> AppResult<()> {
    if let Some(id) = db.setting(ID)? {
        bridge::revoke(&id);
    }
    db.delete_setting(ID)
}
pub(crate) fn enable(db: &Database) -> AppResult<ModelConfig> {
    let state = status(db)?;
    if !state.online {
        return Err(AppError::Validation(
            "请先在本机启动桥接，待显示在线后再启用".into(),
        ));
    }
    let previous = db.model_config()?;
    let selected = if previous.provider == "codex-bridge"
        && state
            .models
            .iter()
            .any(|entry| entry.model == previous.model)
    {
        previous.model.as_str()
    } else {
        state.model.as_deref().unwrap_or_default()
    };
    save(db, selected)
}
pub(crate) fn save(db: &Database, model: &str) -> AppResult<ModelConfig> {
    let state = status(db)?;
    if !state.online {
        return Err(AppError::Validation("本机桥接不在线，请重新连接".into()));
    }
    if !state.models.iter().any(|entry| entry.model == model) {
        return Err(AppError::Validation(
            "此模型不在本机 Codex 返回的模型列表中，请重新选择".into(),
        ));
    }
    db.save_model_metadata("codex-bridge", "codex://bridge", model)?;
    db.model_config()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_selection_is_validated_preserved_and_account_scoped() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let token = pair(&db).unwrap();
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
        assert_eq!(status(&db).unwrap().models, models);
        assert_eq!(enable(&db).unwrap().model, "a");
        assert!(save(&db, "unknown").is_err());
        assert_eq!(save(&db, "b").unwrap().model, "b");
        assert_eq!(enable(&db).unwrap().model, "b");
        assert!(connection
            .advertise(&uuid::Uuid::new_v4().to_string(), "a", &models)
            .is_err());
        assert_eq!(status(&db).unwrap().models, models);
        revoke(&db).unwrap();
        assert!(status(&db).unwrap().models.is_empty());
        assert_eq!(db.model_config().unwrap().model, "b");
    }
    #[tokio::test]
    async fn bridge_account_selection_cannot_be_redirected_by_a_client_model_url() {
        let a = Database::open(std::path::Path::new(":memory:")).unwrap();
        let b = Database::open(std::path::Path::new(":memory:")).unwrap();
        let token_a = pair(&a).unwrap();
        let token_b = pair(&b).unwrap();
        assert_ne!(id(&a).unwrap(), id(&b).unwrap());
        assert!(!status(&a).unwrap().online);
        assert!(enable(&b).is_err());
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
        assert!(status(&a).unwrap().online);
        assert!(!status(&b).unwrap().online);
        // Saving a supplied endpoint cannot borrow another account's connection.
        assert!(save(&b, "fixture").is_err());
        let configured = enable(&a).unwrap();
        assert_eq!(configured.base_url, "codex://bridge");
        let backup = serde_json::to_string(&a.export_sync_data().unwrap()).unwrap();
        assert!(!backup.contains(&id(&a).unwrap()));
        assert!(!backup.contains(&token_a));
        revoke(&b).unwrap();
        assert!(bridge::authenticate(&token_a).is_ok());
        assert!(bridge::authenticate(&token_b).is_err());
        pair(&a).unwrap();
        assert!(bridge::authenticate(&token_a).is_err());
        revoke(&a).unwrap();
        assert!(id(&a).is_err());
        assert!(!a.model_config().unwrap().has_api_key);
        poll.abort();
    }
}
