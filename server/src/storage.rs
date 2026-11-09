//! PostgreSQL boundary: repositories share dynamically typed rows over a dedicated client thread.
//! Each tenant connection pins `search_path` to one account schema.
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "storage_pool_tests.rs"]
mod pool_tests;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("查询没有返回记录")]
    QueryReturnedNoRows,
    #[error("结果中没有第 {0} 列")]
    InvalidColumnIndex(usize),
    #[error("第 {index} 列无法转换：{message}")]
    Conversion { index: usize, message: String },
    #[error("账号数据空间名称无效")]
    InvalidSchema,
    #[error("数据库连接已关闭")]
    Disconnected,
    #[error("{0}")]
    Postgres(#[from] postgres::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

pub trait ToSql {
    fn to_value(&self) -> Value;
}
impl<T: ToSql + ?Sized> ToSql for &T {
    fn to_value(&self) -> Value {
        (**self).to_value()
    }
}
impl ToSql for Value {
    fn to_value(&self) -> Value {
        self.clone()
    }
}
impl ToSql for str {
    fn to_value(&self) -> Value {
        Value::Text(self.to_owned())
    }
}
impl ToSql for String {
    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }
}
impl ToSql for f64 {
    fn to_value(&self) -> Value {
        Value::Real(*self)
    }
}
impl ToSql for bool {
    fn to_value(&self) -> Value {
        Value::Integer((*self).into())
    }
}
macro_rules! integer_to_sql {
    ($($t:ty),*) => { $(impl ToSql for $t {
        fn to_value(&self) -> Value {
            i64::try_from(*self).map_or(Value::Real(*self as f64), Value::Integer)
        }
    })* };
}
integer_to_sql!(i32, i64, u32, u64, usize);
impl<T: ToSql> ToSql for Option<T> {
    fn to_value(&self) -> Value {
        self.as_ref().map_or(Value::Null, ToSql::to_value)
    }
}

pub trait FromSql: Sized {
    fn from_value(value: &Value) -> std::result::Result<Self, String>;
}
fn mismatch(expected: &str, value: &Value) -> String {
    format!("期望 {expected}，实际为 {value:?}")
}
impl FromSql for Value {
    fn from_value(value: &Value) -> std::result::Result<Self, String> {
        Ok(value.clone())
    }
}
impl FromSql for String {
    fn from_value(value: &Value) -> std::result::Result<Self, String> {
        match value {
            Value::Text(text) => Ok(text.clone()),
            other => Err(mismatch("文本", other)),
        }
    }
}
impl FromSql for f64 {
    fn from_value(value: &Value) -> std::result::Result<Self, String> {
        match value {
            Value::Real(number) => Ok(*number),
            Value::Integer(number) => Ok(*number as f64),
            other => Err(mismatch("数值", other)),
        }
    }
}
impl FromSql for bool {
    fn from_value(value: &Value) -> std::result::Result<Self, String> {
        match value {
            Value::Integer(number) => Ok(*number != 0),
            other => Err(mismatch("布尔整数", other)),
        }
    }
}
macro_rules! integer_from_sql {
    ($($t:ty),*) => { $(impl FromSql for $t {
        fn from_value(value: &Value) -> std::result::Result<Self, String> {
            match value {
                Value::Integer(number) => <$t>::try_from(*number).map_err(|error| error.to_string()),
                other => Err(mismatch("整数", other)),
            }
        }
    })* };
}
integer_from_sql!(i32, i64, u32, u64, usize);
impl<T: FromSql> FromSql for Option<T> {
    fn from_value(value: &Value) -> std::result::Result<Self, String> {
        match value {
            Value::Null => Ok(None),
            other => T::from_value(other).map(Some),
        }
    }
}

pub trait Params {
    fn values(self) -> Vec<Value>;
}
impl Params for [&dyn ToSql; 0] {
    fn values(self) -> Vec<Value> {
        vec![]
    }
}
macro_rules! array_params { ($($n:expr),*) => { $(impl<T: ToSql> Params for [T; $n] { fn values(self) -> Vec<Value> { self.iter().map(ToSql::to_value).collect() } })* }; }
array_params!(1, 2, 3, 4, 5, 6, 7, 8);
impl Params for &[&dyn ToSql] {
    fn values(self) -> Vec<Value> {
        self.iter().map(|value| value.to_value()).collect()
    }
}
impl Params for Vec<Value> {
    fn values(self) -> Vec<Value> {
        self
    }
}
macro_rules! params {
    () => { &[] as &[&dyn $crate::storage::ToSql] };
    ($($value:expr),+ $(,)?) => { &[$(&$value as &dyn $crate::storage::ToSql),+] as &[&dyn $crate::storage::ToSql] };
}
pub(crate) use params;

pub trait OptionalRow<T> {
    fn optional(self) -> Result<Option<T>>;
}
impl<T> OptionalRow<T> for Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

pub struct Row {
    values: Vec<Value>,
}
impl Row {
    pub fn get<I: TryInto<usize>, T: FromSql>(&self, index: I) -> Result<T> {
        let index = index
            .try_into()
            .map_err(|_| Error::InvalidColumnIndex(usize::MAX))?;
        T::from_value(self.value(index)?).map_err(|message| Error::Conversion { index, message })
    }
    pub fn value(&self, index: usize) -> Result<&Value> {
        self.values
            .get(index)
            .ok_or(Error::InvalidColumnIndex(index))
    }
}

fn receive<T: Send>(rx: mpsc::Receiver<T>) -> Result<T> {
    if tokio::runtime::Handle::try_current()
        .is_ok_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread)
    {
        tokio::task::block_in_place(|| rx.recv())
    } else {
        rx.recv()
    }
    .map_err(|_| Error::Disconnected)
}

type Reply = mpsc::Sender<Result<(usize, Vec<Row>)>>;
struct Command {
    sql: String,
    values: Vec<Value>,
    batch: bool,
    reply: Reply,
}
pub struct Connection {
    commands: mpsc::Sender<Command>,
    lease: Option<Lease>,
}

struct Lease {
    pool: Weak<PoolInner>,
    schema: String,
}
struct IdleConnection {
    commands: mpsc::Sender<Command>,
    schema: String,
    since: Instant,
}
struct PoolInner {
    url: String,
    max_idle: usize,
    idle: Mutex<Vec<IdleConnection>>,
}

/// API-owned bounded idle connections, never shared across database URLs or tenant schemas.
#[derive(Clone)]
pub(crate) struct ConnectionPool(Arc<PoolInner>);
impl ConnectionPool {
    pub(crate) fn new(url: String, max_idle: usize) -> Self {
        Self(Arc::new(PoolInner {
            url,
            max_idle,
            idle: Mutex::new(Vec::new()),
        }))
    }

    pub(crate) fn acquire(&self, schema: &str) -> Result<Connection> {
        let started = Instant::now();
        let idle = {
            let mut idle = self
                .0
                .idle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            idle.retain(|entry| entry.since.elapsed() < Duration::from_secs(60));
            idle.iter()
                .rposition(|entry| entry.schema == schema)
                .map(|index| idle.swap_remove(index))
        };
        let mut reused = false;
        let mut connection = if let Some(idle) = idle {
            let connection = Connection {
                commands: idle.commands,
                lease: None,
            };
            if connection.execute_batch("SELECT 1").is_ok() {
                reused = true;
                connection
            } else {
                Connection::postgres(&self.0.url, schema)?
            }
        } else {
            Connection::postgres(&self.0.url, schema)?
        };
        connection.lease = Some(Lease {
            pool: Arc::downgrade(&self.0),
            schema: schema.into(),
        });
        tracing::debug!(target: "mario::timing", stage = "db_lease", elapsed_us = started.elapsed().as_micros() as u64, reused);
        Ok(connection)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        let Some(pool) = lease.pool.upgrade() else {
            return;
        };
        let started = Instant::now();
        // DISCARD ALL is deliberately a standalone statement, outside any transaction.
        // It clears advisory locks, temporary objects, prepared statements and session settings.
        let reset = self
            .execute_batch("ROLLBACK")
            .and_then(|_| self.execute_batch("DISCARD ALL"))
            .and_then(|_| {
                self.execute_batch(&format!(
                    "SET statement_timeout TO '30s'; SET search_path TO {}",
                    lease.schema
                ))
            });
        tracing::debug!(target: "mario::timing", stage = "db_reset", elapsed_us = started.elapsed().as_micros() as u64, success = reset.is_ok());
        if reset.is_err() {
            return;
        }
        let mut idle = pool
            .idle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pool.max_idle == 0 {
            return;
        }
        if idle.len() >= pool.max_idle {
            let oldest = idle
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.since)
                .map(|(index, _)| index)
                .unwrap();
            idle.swap_remove(oldest);
        }
        idle.push(IdleConnection {
            commands: self.commands.clone(),
            schema: lease.schema,
            since: Instant::now(),
        });
    }
}
impl Connection {
    pub fn postgres(url: &str, schema: &str) -> Result<Self> {
        if schema.is_empty()
            || !schema
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        {
            return Err(Error::InvalidSchema);
        }
        let (tx, rx) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let url = url.to_owned();
        let schema = schema.to_owned();
        let dispatcher = tracing::dispatcher::get_default(Clone::clone);
        std::thread::spawn(move || {
            tracing::dispatcher::with_default(&dispatcher, || {
                let connected = (|| {
                    let started = std::time::Instant::now();
                    let result = postgres::Client::connect(&url, postgres::NoTls);
                    tracing::debug!(target: "mario::timing", stage = "db_connect", elapsed_us = started.elapsed().as_micros() as u64, success = result.is_ok());
                    let mut c = result?;
                    let started = std::time::Instant::now();
                    let result = c.batch_execute(&format!("SET statement_timeout TO '30s'; SELECT pg_advisory_lock(hashtextextended('{schema}',0)); CREATE SCHEMA IF NOT EXISTS {schema}; SET search_path TO {schema}; SELECT pg_advisory_unlock(hashtextextended('{schema}',0));"));
                    tracing::debug!(target: "mario::timing", stage = "db_bind", elapsed_us = started.elapsed().as_micros() as u64, success = result.is_ok());
                    result?;
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
                    let started = std::time::Instant::now();
                    let result = run(&mut client, &cmd.sql, cmd.values, cmd.batch);
                    tracing::debug!(target: "mario::timing", stage = "sql", elapsed_us = started.elapsed().as_micros() as u64, success = result.is_ok());
                    let _ = cmd.reply.send(result);
                }
            })
        });
        receive(ready_rx)??;
        Ok(Self {
            commands: tx,
            lease: None,
        })
    }
    fn run(&self, sql: &str, values: Vec<Value>, batch: bool) -> Result<(usize, Vec<Row>)> {
        let (reply, rx) = mpsc::channel();
        self.commands
            .send(Command {
                sql: sql.into(),
                values,
                batch,
                reply,
            })
            .map_err(|_| Error::Disconnected)?;
        receive(rx)?
    }
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        self.run(sql, vec![], true).map(|_| ())
    }
    pub fn execute(&self, sql: &str, params: impl Params) -> Result<usize> {
        self.run(sql, params.values(), false).map(|r| r.0)
    }
    pub fn query_row<T>(
        &self,
        sql: &str,
        params: impl Params,
        f: impl FnOnce(&Row) -> Result<T>,
    ) -> Result<T> {
        let (_, rows) = self.run(sql, params.values(), false)?;
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
        mut f: impl FnMut(&Row) -> Result<T>,
    ) -> Result<std::vec::IntoIter<Result<T>>> {
        Ok(self
            .connection
            .run(&self.sql, params.values(), false)?
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
fn run(
    client: &mut postgres::Client,
    sql: &str,
    values: Vec<Value>,
    batch: bool,
) -> Result<(usize, Vec<Row>)> {
    if batch {
        client.batch_execute(sql)?;
        return Ok((0, vec![]));
    }
    let values = values.into_iter().map(PgValue).collect::<Vec<_>>();
    let params = values
        .iter()
        .map(|v| v as &(dyn postgres::types::ToSql + Sync))
        .collect::<Vec<_>>();
    let statement = client.prepare(sql)?;
    if statement.columns().is_empty() {
        return Ok((client.execute(&statement, &params)? as usize, vec![]));
    }
    let rows = client
        .query(&statement, &params)?
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
                        Type::INT2 => r
                            .try_get::<_, Option<i16>>(i)
                            .map(|v| v.map(|v| Value::Integer(v.into()))),
                        Type::FLOAT8 => r.try_get::<_, Option<f64>>(i).map(|v| v.map(Value::Real)),
                        Type::BOOL => r
                            .try_get::<_, Option<bool>>(i)
                            .map(|v| v.map(|v| Value::Integer(v.into()))),
                        _ => r
                            .try_get::<_, Option<String>>(i)
                            .map(|v| v.map(Value::Text)),
                    }?;
                    Ok(v.unwrap_or(Value::Null))
                })
                .collect::<Result<_>>()?;
            Ok(Row { values })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((rows.len(), rows))
}
