#[cfg(test)]
mod tests {
    use super::super::*;

    fn fixture(max_idle: usize) -> (ConnectionPool, String, String) {
        let account = crate::db::TestAccount::new();
        let conn = account.connection();
        let schema = conn
            .query_row("SELECT current_schema()::text", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap();
        let url = std::env::var("MARIO_TEST_DATABASE_URL").unwrap();
        (ConnectionPool::new(url.clone(), max_idle), schema, url)
    }
    fn pid(connection: &Connection) -> i32 {
        connection
            .query_row("SELECT pg_backend_pid()", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn reuse_rolls_back_and_clears_locks_temp_objects_and_session_configuration() {
        let (pool, schema, url) = fixture(2);
        let connection = pool.acquire(&schema).unwrap();
        let original = pid(&connection);
        connection.execute_batch("CREATE TABLE saved(value BIGINT); INSERT INTO saved VALUES(1); CREATE TEMP TABLE temporary_secret(value TEXT)").unwrap();
        connection.execute_batch(&format!("SELECT pg_advisory_lock(hashtextextended('{schema}',0)); SET statement_timeout='1s'; SET search_path TO public; BEGIN; INSERT INTO {schema}.saved VALUES(2)")).unwrap();
        drop(connection);
        let connection = pool.acquire(&schema).unwrap();
        assert_eq!(pid(&connection), original);
        assert_eq!(
            connection
                .query_row("SELECT current_schema()::text", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            schema
        );
        assert_eq!(
            connection
                .query_row("SHOW statement_timeout", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "30s"
        );
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM saved", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(connection
            .query_row(
                "SELECT to_regclass('pg_temp.temporary_secret')::text",
                [],
                |r| r.get::<_, Option<String>>(0)
            )
            .unwrap()
            .is_none());
        let peer = Connection::postgres(&url, &schema).unwrap();
        assert!(peer
            .query_row(
                &format!("SELECT pg_try_advisory_lock(hashtextextended('{schema}',0))"),
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap());
        peer.execute_batch("SELECT pg_advisory_unlock_all()")
            .unwrap();
    }

    #[test]
    fn leases_are_exclusive_tenant_bound_and_idle_capacity_is_bounded() {
        let (pool, schema, _) = fixture(1);
        let first = pool.acquire(&schema).unwrap();
        let second = pool.acquire(&schema).unwrap();
        assert_ne!(pid(&first), pid(&second));
        first.execute_batch("CREATE TABLE private_values(value TEXT); INSERT INTO private_values VALUES('private')").unwrap();
        let other = format!("user_{}", uuid::Uuid::new_v4().simple());
        let other_connection = pool.acquire(&other).unwrap();
        assert_eq!(
            other_connection
                .query_row("SELECT current_schema()::text", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            other
        );
        assert!(other_connection
            .query_row("SELECT to_regclass('private_values')::text", [], |r| r
                .get::<_, Option<
                String,
            >>(
                0
            ))
            .unwrap()
            .is_none());
        let recent = pid(&other_connection);
        drop(first);
        drop(second);
        drop(other_connection);
        assert_eq!(pool.0.idle.lock().unwrap().len(), 1);
        assert_eq!(pid(&pool.acquire(&other).unwrap()), recent);
        assert!(pool.acquire("invalid-schema; SELECT 1").is_err());
    }

    #[test]
    fn broken_idle_or_active_connections_are_discarded_and_replaced() {
        let (pool, schema, url) = fixture(2);
        let peer = Connection::postgres(&url, &schema).unwrap();
        let connection = pool.acquire(&schema).unwrap();
        let original = pid(&connection);
        drop(connection);
        assert!(peer
            .query_row("SELECT pg_terminate_backend($1)", [original], |r| r
                .get::<_, bool>(0))
            .unwrap());
        let connection = pool.acquire(&schema).unwrap();
        let replacement = pid(&connection);
        assert_ne!(replacement, original);
        assert!(peer
            .query_row("SELECT pg_terminate_backend($1)", [replacement], |r| r
                .get::<_, bool>(0))
            .unwrap());
        drop(connection);
        assert!(pool.0.idle.lock().unwrap().is_empty());
        assert_ne!(pid(&pool.acquire(&schema).unwrap()), replacement);
    }

    #[test]
    fn failed_transactions_are_rolled_back_before_reuse() {
        let (pool, schema, _) = fixture(1);
        let connection = pool.acquire(&schema).unwrap();
        let original = pid(&connection);
        assert!(connection
            .execute_batch("BEGIN; SELECT 'invalid'::BIGINT")
            .is_err());
        drop(connection);
        let connection = pool.acquire(&schema).unwrap();
        assert_eq!(pid(&connection), original);
        assert_eq!(
            connection
                .query_row("SELECT 42", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            42
        );
    }
}
