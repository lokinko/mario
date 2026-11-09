//! Isolated, explicit measurements; never use a deployed database or timing assertions.
use crate::db::{Database, TestAccount};
use std::time::Instant;

#[test]
fn timing_events_keep_sql_parameters_and_connection_credentials_out_of_logs() {
    const CHILD: &str = "MARIO_TIMING_CAPTURE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // Production installs a global subscriber before opening connections. Exercise
        // that lifecycle in a child so parallel tests cannot seed disabled callsites
        // or have their own SQL included in this test's privacy assertions.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "performance_tests::timing_events_keep_sql_parameters_and_connection_credentials_out_of_logs",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "timing capture failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    #[derive(Clone)]
    struct Writer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let capture = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_env_filter("mario::timing=debug")
        .with_writer(move || Writer(capture.clone()))
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let account = TestAccount::new();
    let db = account.open();
    {
        let _lock = db.lock_account().unwrap();
        let private = "synthetic-private-parameter";
        assert_eq!(
            db.conn()
                .unwrap()
                .query_row("SELECT $1::TEXT", [private], |r| r.get::<_, String>(0))
                .unwrap(),
            private
        );
    }
    drop(db);
    let output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    for stage in ["db_connect", "db_bind", "sql", "account_lock"] {
        assert!(
            output.contains(stage),
            "missing timing stage {stage}; captured: {output}"
        );
    }
    assert!(!output.contains("synthetic-private-parameter"));
    assert!(!output.contains("SELECT $1"));
    assert!(!output.contains(&std::env::var("MARIO_TEST_DATABASE_URL").unwrap()));
}

fn read(db: &Database, phases: &mut Vec<(u64, u64, u64)>) {
    let started = Instant::now();
    let _lock = db.lock_account().unwrap();
    let lock_us = started.elapsed().as_micros() as u64;
    let started = Instant::now();
    let snapshot = db.snapshot().unwrap();
    let snapshot_us = started.elapsed().as_micros() as u64;
    let started = Instant::now();
    let bytes = serde_json::to_vec(&snapshot).unwrap();
    assert!(!bytes.is_empty());
    phases.push((lock_us, snapshot_us, started.elapsed().as_micros() as u64));
}

fn percentiles(mut samples: Vec<u64>) -> serde_json::Value {
    samples.sort_unstable();
    serde_json::json!({ "p50Us": samples[samples.len() / 2], "p95Us": samples[(samples.len() * 95 / 100).min(samples.len()-1)] })
}

#[test]
#[ignore = "explicit disposable-database measurement: npm run measure:storage"]
fn storage_roundtrip_measurement() {
    let account = TestAccount::new();
    let reused = account.open();
    for index in 0..20 {
        let holding = serde_json::from_value(serde_json::json!({
            "name":format!("isolated-{index}"), "symbol":"", "assetClass":"股票",
            "marketValue":10000, "costBasis":9000, "targetPct":0,
            "currency":"CNY", "valuationDate":"2026-01-02"
        }))
        .unwrap();
        reused.add_holding(&holding).unwrap();
    }
    let mut fresh = Vec::new();
    let mut repeated = Vec::new();
    let mut connects = Vec::new();
    let mut phases = Vec::new();
    let pool =
        crate::storage::ConnectionPool::new(std::env::var("MARIO_TEST_DATABASE_URL").unwrap(), 8);
    let mut pooled = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        let db = account.open();
        connects.push(started.elapsed().as_micros() as u64);
        read(&db, &mut phases);
        fresh.push(started.elapsed().as_micros() as u64);
        let started = Instant::now();
        read(&reused, &mut phases);
        repeated.push(started.elapsed().as_micros() as u64);
        let started = Instant::now();
        {
            let db = account.pooled(&pool);
            read(&db, &mut phases);
        }
        pooled.push(started.elapsed().as_micros() as u64);
    }
    println!(
        "MARIO_STORAGE_MEASUREMENT:{}",
        serde_json::json!({
            "holdings":20, "readsPerMode":30, "environment":"isolated loopback PostgreSQL",
            "freshConnectionRead":percentiles(fresh), "reusedConnectionRead":percentiles(repeated),
            "connectionAndSchemaCheck":percentiles(connects),
            "pooledRequestIncludingReset":percentiles(pooled),
            "accountLock":percentiles(phases.iter().map(|v|v.0).collect()),
            "snapshot":percentiles(phases.iter().map(|v|v.1).collect()),
            "serialization":percentiles(phases.iter().map(|v|v.2).collect()),
        })
    );
}
