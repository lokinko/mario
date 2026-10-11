use super::*;

struct Tenant {
    url: String,
    user: String,
    schema: String,
    admin: postgres::Client,
}

impl Tenant {
    fn legacy() -> Self {
        let url = std::env::var("MARIO_TEST_DATABASE_URL")
            .expect("set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database");
        let id = Uuid::new_v4();
        let user = id.to_string();
        let schema = format!("user_{}", id.simple());
        let admin = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        let mut tenant = Self {
            url,
            user,
            schema,
            admin,
        };
        let db = Database::postgres(&tenant.url, &tenant.user).unwrap();
        let profile = FinancialProfile {
            monthly_income: 12345.0,
            ..Default::default()
        };
        db.save_profile(&profile).unwrap();
        db.conn()
            .unwrap()
            .execute(
                "INSERT INTO settings(key,value) VALUES ('migration-test','keep-me')",
                [],
            )
            .unwrap();
        drop(db);
        // Old releases already have revision tracking, but not the newer table/column.
        tenant.sql(
            "DROP TABLE research_attachments;
                    ALTER TABLE decision_reviews DROP COLUMN content;
                    ALTER TABLE data_revision DROP COLUMN schema_version;",
        );
        tenant
    }

    fn sql(&mut self, sql: &str) {
        self.admin
            .batch_execute(&format!("SET search_path TO {}; {sql}", self.schema))
            .unwrap();
    }

    fn check(&mut self, sql: &str) -> bool {
        self.admin.query_one(sql, &[]).unwrap().get(0)
    }
}

impl Drop for Tenant {
    fn drop(&mut self) {
        let _ = self
            .admin
            .batch_execute(&format!("DROP SCHEMA {} CASCADE", self.schema));
    }
}

#[test]
#[ignore = "requires MARIO_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
fn postgres_migrations_upgrade_existing_tenant_once_across_replicas() {
    let mut tenant = Tenant::legacy();
    let before: i64 = tenant
        .admin
        .query_one("SELECT revision FROM data_revision WHERE id=1", &[])
        .unwrap()
        .get(0);
    let handles = (0..2).map(|_| {
        let url = tenant.url.clone();
        let user = tenant.user.clone();
        std::thread::spawn(move || {
            let db = Database::postgres(&url, &user).unwrap();
            assert_eq!(db.snapshot().unwrap().profile.monthly_income, 12345.0);
            assert_eq!(db.revision().unwrap(), before.to_string());
            assert_eq!(db.conn().unwrap().query_row(
                "SELECT value FROM settings WHERE key='migration-test'", [], |r| r.get::<_, String>(0)
            ).unwrap(), "keep-me");
            // These are the two queries that failed on the deployed server.
            db.conn().unwrap().prepare("SELECT evidence_id, id, name, byte_size, extracted_text, extraction_note FROM research_attachments ORDER BY id").unwrap()
                .query_map([], |_| Ok(())).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
            db.conn().unwrap().prepare("SELECT r.content FROM decisions d LEFT JOIN decision_reviews r ON r.decision_id=d.id").unwrap()
                .query_map([], |_| Ok(())).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        })
    }).collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(tenant.check(&format!(
        "SELECT schema_version={} FROM data_revision WHERE id=1",
        schema::POSTGRES_SCHEMA_VERSION
    )));
    assert!(tenant.check("SELECT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid='research_attachments'::regclass AND tgname='data_changed')"));
    // Opening a current schema must skip DDL entirely.
    tenant.sql(
        "CREATE FUNCTION reject_repeat_migration() RETURNS trigger LANGUAGE plpgsql AS $$
                BEGIN RAISE EXCEPTION 'migration unexpectedly repeated'; END; $$;
                CREATE TRIGGER reject_repeat_migration BEFORE INSERT ON data_revision
                FOR EACH ROW EXECUTE FUNCTION reject_repeat_migration();",
    );
    let db = Database::postgres(&tenant.url, &tenant.user).unwrap();
    assert_eq!(db.revision().unwrap(), before.to_string());
    drop(db);
}

#[test]
#[ignore = "requires MARIO_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
fn postgres_migrations_roll_back_failed_upgrade_and_allow_retry() {
    let mut tenant = Tenant::legacy();
    tenant.sql(
        "CREATE FUNCTION reject_schema_stamp() RETURNS trigger LANGUAGE plpgsql AS $$
                BEGIN RAISE EXCEPTION 'test migration failure'; END; $$;
                CREATE TRIGGER reject_schema_stamp BEFORE UPDATE ON data_revision
                FOR EACH ROW EXECUTE FUNCTION reject_schema_stamp();",
    );
    let error = Database::postgres(&tenant.url, &tenant.user)
        .err()
        .expect("upgrade must fail");
    assert!(format!("{error:?}").contains("test migration failure"));
    assert!(tenant.check("SELECT to_regclass('research_attachments') IS NULL"));
    assert!(!tenant.check("SELECT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid='decision_reviews'::regclass AND attname='content' AND NOT attisdropped)"));
    assert!(!tenant.check("SELECT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid='data_revision'::regclass AND attname='schema_version' AND NOT attisdropped)"));
    tenant.sql(
        "DROP TRIGGER reject_schema_stamp ON data_revision; DROP FUNCTION reject_schema_stamp();",
    );
    let db = Database::postgres(&tenant.url, &tenant.user).unwrap();
    assert_eq!(db.snapshot().unwrap().profile.monthly_income, 12345.0);
    drop(db);
}

#[test]
#[ignore = "requires MARIO_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
fn postgres_migrates_version_one_holdings_and_persists_manual_unit_prices() {
    let mut tenant = Tenant::legacy();
    drop(Database::postgres(&tenant.url, &tenant.user).unwrap());
    tenant.sql("ALTER TABLE holdings DROP COLUMN cost_price; ALTER TABLE holdings DROP COLUMN quantity; ALTER TABLE holdings DROP COLUMN current_price; UPDATE data_revision SET schema_version=1 WHERE id=1;");
    let db = Database::postgres(&tenant.url, &tenant.user).unwrap();
    let input: HoldingInput = serde_json::from_value(serde_json::json!({
        "symbol":"IDX", "name":"手动持仓", "assetClass":"基金", "marketValue":0,
        "costBasis":0, "costPrice":1.2345, "quantity":100.5, "currentPrice":1.4567,
        "targetPct":0, "currency":"CNY", "valuationDate":"2026-01-01"
    }))
    .unwrap();
    let snapshot = db.add_holding(&input).unwrap();
    let holding = &snapshot.holdings[0];
    assert_eq!(holding.cost_price, Some(1.2345));
    assert_eq!(holding.quantity, Some(100.5));
    assert_eq!(holding.current_price, Some(1.4567));
    assert_eq!(holding.market_value, 100.5 * 1.4567);
    assert_eq!(holding.cost_basis, 100.5 * 1.2345);
    let exported = db.export_sync_data().unwrap();
    db.import_sync_data(&exported).unwrap();
    drop(db);
    assert_eq!(
        Database::postgres(&tenant.url, &tenant.user)
            .unwrap()
            .snapshot()
            .unwrap()
            .holdings[0]
            .current_price,
        Some(1.4567)
    );
}
