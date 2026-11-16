#[cfg(test)]
mod tests {
    use super::super::*;
    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use std::sync::Arc;
    use tokio::sync::{oneshot, Mutex, Notify};

    struct Model {
        entered: Notify,
        gate: Mutex<Option<oneshot::Receiver<()>>>,
        fail: bool,
    }
    async fn reply(
        State(model): State<Arc<Model>>,
        Json(body): Json<serde_json::Value>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        assert_eq!(body["model"], "frozen-model");
        model.entered.notify_one();
        if let Some(gate) = model.gate.lock().await.take() {
            let _ = gate.await;
        }
        if model.fail {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":{"message":"synthetic failure"}})),
            )
        } else {
            (
                StatusCode::OK,
                Json(
                    serde_json::json!({"choices":[{"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}]}),
                ),
            )
        }
    }
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    fn peer_can_lock(db: &Database) -> bool {
        let conn = db.conn().unwrap();
        let acquired = conn
            .query_row(
                "SELECT pg_try_advisory_lock(hashtextextended(current_schema(),0))",
                [],
                |row| row.get::<_, bool>(0),
            )
            .unwrap();
        if acquired {
            conn.execute_batch("SELECT pg_advisory_unlock(hashtextextended(current_schema(),0))")
                .unwrap();
        }
        acquired
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connection_test_unlocks_and_rejects_changed_models_or_credentials_without_writes() {
        crate::secrets::use_test_master_key();
        for case in 0..5 {
            let account = crate::db::TestAccount::new();
            let db = account.open();
            let peer = account.open();
            let (release, gate) = oneshot::channel();
            let model = Arc::new(Model {
                entered: Notify::new(),
                gate: Mutex::new(Some(gate)),
                fail: case == 3,
            });
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let app = Router::new()
                .route("/chat/completions", post(reply))
                .with_state(model.clone());
            let _server = Server(tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            }));
            db.save_model_configuration(
                ModelProviderKind::OpenaiChat,
                &url,
                "frozen-model",
                Some("synthetic-key"),
            )
            .unwrap();
            let outer = db.lock_account().unwrap();
            let mut work = Box::pin(test_connection(
                &db,
                &crate::ai::management::ModelConnectorAdapter,
                None,
            ));
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::select! { _ = model.entered.notified() => {}, result = work.as_mut() => panic!("model did not wait: {result:?}") }
        }).await.unwrap();
            assert!(peer_can_lock(&peer));
            {
                let _lock = peer.lock_account().unwrap();
                if case == 1 || case == 2 {
                    peer.save_model_configuration(
                        ModelProviderKind::OpenaiChat,
                        &url,
                        if case == 1 {
                            "new-model"
                        } else {
                            "frozen-model"
                        },
                        Some("new-key"),
                    )
                    .unwrap();
                } else if case == 0 {
                    let mut profile = peer.snapshot().unwrap().profile;
                    profile.monthly_income = 1234.0;
                    peer.save_profile(&profile).unwrap();
                }
            }
            let revision = peer.revision().unwrap();
            if case == 4 {
                drop(work);
                assert!(peer_can_lock(&peer));
            } else {
                release.send(()).unwrap();
                let result = work.await;
                match case {
                    0 => assert_eq!(result.unwrap().model, "frozen-model"),
                    1 | 2 => assert!(matches!(result, Err(AppError::Conflict(_)))),
                    _ => assert!(result.is_err()),
                }
                assert!(!peer_can_lock(&peer));
            }
            assert_eq!(db.revision().unwrap(), revision);
            drop(outer);
            assert!(peer_can_lock(&peer));
        }
    }
    struct WaitingCatalog {
        entered: Notify,
        gate: Mutex<Option<oneshot::Receiver<()>>>,
    }
    #[async_trait::async_trait]
    impl ModelCatalog for WaitingCatalog {
        async fn list(&self, key: &str) -> AppResult<Vec<String>> {
            assert_eq!(key, "site-catalog-key");
            self.entered.notify_one();
            if let Some(gate) = self.gate.lock().await.take() {
                let _ = gate.await;
            }
            Ok(vec!["catalog-fixture".into()])
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn catalog_selection_unlocks_and_rejects_a_stale_write_after_network_wait() {
        for changed in [false, true] {
            let account = crate::db::TestAccount::new();
            let db = account.open();
            let peer = account.open();
            let (release, gate) = oneshot::channel();
            let catalog = Arc::new(WaitingCatalog {
                entered: Notify::new(),
                gate: Mutex::new(Some(gate)),
            });
            let services = ModelServices {
                connector: Arc::new(crate::ai::management::ModelConnectorAdapter),
                bridges: Arc::new(crate::ai::management::BridgeRegistryAdapter),
                catalog: catalog.clone(),
                server: Some(
                    ServerOpenAi::new("default-model".into(), "site-catalog-key".into()).unwrap(),
                ),
            };
            let outer = db.lock_account().unwrap();
            let mut pending = Box::pin(save(
                &db,
                ModelConfigInput {
                    source: ModelConfigSource::Server,
                    provider: ModelProviderKind::OpenaiResponses,
                    base_url: OPENAI_URL.into(),
                    model: "catalog-fixture".into(),
                    api_key: None,
                },
                &services,
            ));
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                tokio::select! { _ = catalog.entered.notified() => {}, result = pending.as_mut() => panic!("catalog did not wait: {result:?}") }
            }).await.unwrap();
            assert!(peer_can_lock(&peer));
            if changed {
                let _lock = peer.lock_account().unwrap();
                peer.set_setting("unrelated.pending", "changed").unwrap();
            }
            release.send(()).unwrap();
            let result = pending.await;
            if changed {
                assert!(matches!(result, Err(AppError::Conflict(_))));
                assert!(db.setting("model.server_name").unwrap().is_none());
            } else {
                assert_eq!(result.unwrap().model, "catalog-fixture");
            }
            assert!(!peer_can_lock(&peer));
            drop(outer);
            assert!(peer_can_lock(&peer));
        }
    }
}
