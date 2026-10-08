//! Small SQL boundary: domain repositories share queries, not a database engine.
//! PostgreSQL owns hosted data. SQLite is retained only for local data migration/tests.
use rusqlite::{
    types::{FromSql, ToSqlOutput, Value, ValueRef},
    Error, Result,
};
use std::{path::Path, sync::mpsc};
fn receive<T: Send>(rx: mpsc::Receiver<T>) -> std::result::Result<T, mpsc::RecvError> {
    if tokio::runtime::Handle::try_current()
        .is_ok_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread)
    {
        tokio::task::block_in_place(|| rx.recv())
    } else {
        rx.recv()
    }
}

pub trait Params {
    fn values(self) -> Result<Vec<Value>>;
}
fn value(v: &dyn rusqlite::ToSql) -> Result<Value> {
    Ok(match v.to_sql()? {
        ToSqlOutput::Borrowed(v) => v.into(),
        ToSqlOutput::Owned(v) => v,
        _ => return Err(Error::InvalidQuery),
    })
}
impl Params for [&dyn rusqlite::ToSql; 0] {
    fn values(self) -> Result<Vec<Value>> {
        Ok(vec![])
    }
}
macro_rules! array_params { ($($n:expr),*) => { $(impl<T: rusqlite::ToSql> Params for [T; $n] { fn values(self) -> Result<Vec<Value>> { self.iter().map(|v| value(v)).collect() } })* }; }
array_params!(1, 2, 3, 4, 5, 6, 7, 8);
impl Params for &[&dyn rusqlite::ToSql] {
    fn values(self) -> Result<Vec<Value>> {
        self.iter().map(|v| value(*v)).collect()
    }
}
impl Params for Vec<Value> {
    fn values(self) -> Result<Vec<Value>> {
        Ok(self)
    }
}

pub struct Row<'a> {
    values: Vec<Value>,
    marker: std::marker::PhantomData<&'a ()>,
}
impl Row<'_> {
    pub fn get<I: TryInto<usize>, T: FromSql>(&self, index: I) -> Result<T> {
        let i = index.try_into().map_err(|_| Error::InvalidQuery)?;
        T::column_result(self.get_ref(i)?).map_err(|e| {
            Error::FromSqlConversionFailure(i, self.values[i].data_type(), Box::new(e))
        })
    }
    pub fn get_ref(&self, index: usize) -> Result<ValueRef<'_>> {
        self.values
            .get(index)
            .map(ValueRef::from)
            .ok_or(Error::InvalidColumnIndex(index))
    }
}

type Reply = mpsc::Sender<Result<(usize, Vec<Row<'static>>)>>;
pub(crate) struct Command {
    sql: String,
    values: Vec<Value>,
    batch: bool,
    reply: Reply,
}
pub enum Connection {
    Sqlite(rusqlite::Connection),
    Postgres(mpsc::Sender<Command>),
}
fn pg_error(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::ToSqlConversionFailure(Box::new(error))
}
impl Connection {
    pub fn open(path: &Path) -> Result<Self> {
        rusqlite::Connection::open(path).map(Self::Sqlite)
    }
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        rusqlite::Connection::open_in_memory().map(Self::Sqlite)
    }
    pub fn is_postgres(&self) -> bool {
        matches!(self, Self::Postgres(_))
    }
    pub fn postgres(url: &str, schema: &str) -> Result<Self> {
        if schema.is_empty()
            || !schema
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        {
            return Err(Error::InvalidQuery);
        }
        let (tx, rx) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let url = url.to_owned();
        let schema = schema.to_owned();
        std::thread::spawn(move || {
            let connected = (|| {
                let mut c = postgres::Client::connect(&url, postgres::NoTls).map_err(pg_error)?;
                c.batch_execute(&format!("SET statement_timeout TO '30s'; SELECT pg_advisory_lock(hashtextextended('{schema}',0)); CREATE SCHEMA IF NOT EXISTS {schema}; SET search_path TO {schema}; SELECT pg_advisory_unlock(hashtextextended('{schema}',0));")).map_err(pg_error)?;
                Ok::<_, Error>(c)
            })();
            let mut client = match connected {
                Ok(c) => {
                    let _ = ready_tx.send(Ok(()));
                    c
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            for cmd in rx {
                let result = pg_run(&mut client, &cmd.sql, cmd.values, cmd.batch);
                let _ = cmd.reply.send(result);
            }
        });
        receive(ready_rx).map_err(pg_error)??;
        Ok(Self::Postgres(tx))
    }
    fn run(
        &self,
        sql: &str,
        values: Vec<Value>,
        batch: bool,
    ) -> Result<(usize, Vec<Row<'static>>)> {
        match self {
            Self::Sqlite(c) => {
                if batch {
                    c.execute_batch(sql)?;
                    return Ok((0, vec![]));
                }
                let mut statement = c.prepare(sql)?;
                if statement.column_count() == 0 {
                    return Ok((
                        statement.execute(rusqlite::params_from_iter(values))?,
                        vec![],
                    ));
                }
                let count = statement.column_count();
                let rows = statement
                    .query_map(rusqlite::params_from_iter(values), |r| {
                        Ok(Row {
                            values: (0..count).map(|i| r.get(i)).collect::<Result<_>>()?,
                            marker: std::marker::PhantomData,
                        })
                    })?
                    .collect::<Result<Vec<_>>>()?;
                Ok((rows.len(), rows))
            }
            Self::Postgres(tx) => {
                let (reply, rx) = mpsc::channel();
                tx.send(Command {
                    sql: sql.into(),
                    values,
                    batch,
                    reply,
                })
                .map_err(|_| Error::InvalidQuery)?;
                receive(rx).map_err(pg_error)?
            }
        }
    }
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        self.run(sql, vec![], true).map(|_| ())
    }
    pub fn execute(&self, sql: &str, params: impl Params) -> Result<usize> {
        self.run(sql, params.values()?, false).map(|r| r.0)
    }
    pub fn query_row<T>(
        &self,
        sql: &str,
        params: impl Params,
        f: impl FnOnce(&Row<'_>) -> Result<T>,
    ) -> Result<T> {
        let (_, rows) = self.run(sql, params.values()?, false)?;
        f(rows.first().ok_or(Error::QueryReturnedNoRows)?)
    }
    pub fn prepare<'a>(&'a self, sql: &str) -> Result<Statement<'a>> {
        Ok(Statement {
            connection: self,
            sql: sql.into(),
        })
    }
    pub fn transaction(&mut self) -> Result<Transaction<'_>> {
        self.execute_batch("BEGIN")?;
        Ok(Transaction {
            connection: self,
            committed: false,
        })
    }
}
pub struct Statement<'a> {
    connection: &'a Connection,
    sql: String,
}
impl Statement<'_> {
    pub fn execute(&mut self, params: impl Params) -> Result<usize> {
        self.connection.execute(&self.sql, params)
    }
    pub fn query_map<T>(
        &mut self,
        params: impl Params,
        mut f: impl FnMut(&Row<'_>) -> Result<T>,
    ) -> Result<std::vec::IntoIter<Result<T>>> {
        Ok(self
            .connection
            .run(&self.sql, params.values()?, false)?
            .1
            .iter()
            .map(&mut f)
            .collect::<Vec<_>>()
            .into_iter())
    }
}
pub struct Transaction<'a> {
    connection: &'a Connection,
    committed: bool,
}
impl std::ops::Deref for Transaction<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.connection
    }
}
impl Transaction<'_> {
    pub fn commit(mut self) -> Result<()> {
        self.connection.execute_batch("COMMIT")?;
        self.committed = true;
        Ok(())
    }
}
impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.connection.execute_batch("ROLLBACK");
        }
    }
}

#[derive(Debug)]
struct PgValue(Value);
impl postgres::types::ToSql for PgValue {
    fn to_sql(
        &self,
        ty: &postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    {
        use postgres::types::{IsNull, Type};
        match &self.0 {
            Value::Null => Ok(IsNull::Yes),
            Value::Text(v) => v.to_sql(ty, out),
            Value::Blob(v) => v.to_sql(ty, out),
            Value::Integer(v) => match *ty {
                Type::INT4 => i32::try_from(*v)?.to_sql(ty, out),
                Type::INT2 => i16::try_from(*v)?.to_sql(ty, out),
                Type::FLOAT8 => (*v as f64).to_sql(ty, out),
                _ => v.to_sql(ty, out),
            },
            Value::Real(v) => v.to_sql(ty, out),
        }
    }
    fn accepts(_: &postgres::types::Type) -> bool {
        true
    }
    postgres::types::to_sql_checked!();
}
fn dialect(sql: &str) -> String {
    let mut sql = sql
        .replace("PRAGMA journal_mode=WAL;", "")
        .replace("PRAGMA foreign_keys=ON;", "")
        .replace(
            "PRAGMA defer_foreign_keys=ON;",
            "SET CONSTRAINTS ALL DEFERRED;",
        );
    sql = sql.replace(" REAL", " DOUBLE PRECISION");
    if sql.contains("CREATE TABLE") || sql.contains("ADD COLUMN") {
        for target in [
            "decisions(id)",
            "system_reviews(id)",
            "investment_rules(id)",
            "holdings(id)",
            "portfolio_events(id)",
        ] {
            let with_delete = format!("REFERENCES {target} ON DELETE CASCADE");
            sql = sql.replace(
                &with_delete,
                &format!("{with_delete} DEFERRABLE INITIALLY IMMEDIATE"),
            );
            let plain = format!("REFERENCES {target}");
            sql = sql.replace(
                &format!("{plain},"),
                &format!("{plain} DEFERRABLE INITIALLY IMMEDIATE,"),
            );
            sql = sql.replace(
                &format!("{plain})"),
                &format!("{plain} DEFERRABLE INITIALLY IMMEDIATE)"),
            );
            sql = sql.replace(
                &format!("{plain}\n"),
                &format!("{plain} DEFERRABLE INITIALLY IMMEDIATE\n"),
            );
        }
    }
    // Repository SQL is static; parameters are always bound, never interpolated.
    for i in (1..=64).rev() {
        sql = sql.replace(&format!("?{i}"), &format!("${i}"));
    }
    for i in 1..=3 {
        sql = sql.replace(&format!("${i} IS NULL"), &format!("${i}::text IS NULL"));
    }
    if sql.contains("INSERT OR IGNORE") {
        sql = format!(
            "{} ON CONFLICT DO NOTHING",
            sql.replace("INSERT OR IGNORE", "INSERT")
                .trim_end_matches(';')
        );
    }
    sql
}
fn pg_run(
    client: &mut postgres::Client,
    sql: &str,
    values: Vec<Value>,
    batch: bool,
) -> Result<(usize, Vec<Row<'static>>)> {
    let sql = dialect(sql);
    if batch {
        client.batch_execute(&sql).map_err(pg_error)?;
        return Ok((0, vec![]));
    }
    let values = values.into_iter().map(PgValue).collect::<Vec<_>>();
    let params = values
        .iter()
        .map(|v| v as &(dyn postgres::types::ToSql + Sync))
        .collect::<Vec<_>>();
    let statement = client.prepare(&sql).map_err(pg_error)?;
    if statement.columns().is_empty() {
        return client
            .execute(&statement, &params)
            .map(|n| (n as usize, vec![]))
            .map_err(pg_error);
    }
    let rows = client
        .query(&statement, &params)
        .map_err(pg_error)?
        .into_iter()
        .map(|r| {
            use postgres::types::Type;
            let values = r
                .columns()
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let v = match *c.type_() {
                        Type::INT8 => r
                            .try_get::<_, Option<i64>>(i)
                            .map(|v| v.map(Value::Integer)),
                        Type::INT4 => r
                            .try_get::<_, Option<i32>>(i)
                            .map(|v| v.map(|v| Value::Integer(v.into()))),
                        Type::FLOAT8 => r.try_get::<_, Option<f64>>(i).map(|v| v.map(Value::Real)),
                        Type::BOOL => r
                            .try_get::<_, Option<bool>>(i)
                            .map(|v| v.map(|v| Value::Integer(v.into()))),
                        _ => r
                            .try_get::<_, Option<String>>(i)
                            .map(|v| v.map(Value::Text)),
                    }
                    .map_err(pg_error)?;
                    Ok(v.unwrap_or(Value::Null))
                })
                .collect::<Result<_>>()?;
            Ok(Row {
                values,
                marker: std::marker::PhantomData,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((rows.len(), rows))
}
