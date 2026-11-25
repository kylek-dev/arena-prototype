//! The account store (ADR 0055): a PostgreSQL database of its own (never
//! shared with other data) holding the minimum about each account. The
//! schema is created and upgraded by numbered migrations on start.

use std::collections::HashSet;
use std::time::Duration;

use arena_protocol::{Handle, MAX_HANDLE_NUMBER};
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use tokio_postgres::{NoTls, Row};

/// An account as sessions see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    /// Our random id, never 0.
    pub id: u64,
    /// Raised to end every session at once (log out everywhere, bans).
    pub generation: u32,
    /// The chosen `Name#1234`, once picked.
    pub handle: Option<Handle>,
    /// When the handle last changed (Unix seconds).
    pub renamed: Option<u64>,
}

/// The shortest time between handle changes (the first pick is free).
pub const RENAME_EVERY: u64 = 7 * 24 * 60 * 60;

/// Why a handle could not be set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleError {
    /// The account does not exist (any more).
    NoAccount,
    /// Renamed too recently; allowed again at this Unix second.
    TooSoon(u64),
    /// Every number of this name is taken.
    Full,
}

/// A database failure. Its text never holds account data, only what went
/// wrong (connection, query or constraint).
#[derive(Debug)]
pub struct StoreError(String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

impl From<tokio_postgres::Error> for StoreError {
    fn from(error: tokio_postgres::Error) -> Self {
        Self(format!("database: {error}"))
    }
}

impl From<deadpool_postgres::PoolError> for StoreError {
    fn from(error: deadpool_postgres::PoolError) -> Self {
        Self(format!("database pool: {error}"))
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

/// Where the accounts live: a `postgres://` URL (from
/// `ARENA_DATABASE_URL`) and, for tests, a schema to use inside it.
#[derive(Clone, PartialEq, Eq)]
pub struct Database {
    pub url: String,
    pub schema: Option<String>,
}

impl std::fmt::Debug for Database {
    // The URL holds the password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("url", &"..")
            .field("schema", &self.schema)
            .finish()
    }
}

/// Schema changes, applied in order once each. Never edit a released one:
/// add the next.
const MIGRATIONS: &[(i32, &str)] = &[(
    1,
    "
    CREATE TABLE accounts (
        id BIGINT PRIMARY KEY,
        -- The WorkOS user id (\"user_...\"); WorkOS holds the email.
        workos_user TEXT NOT NULL UNIQUE,
        generation INTEGER NOT NULL DEFAULT 0,
        created BIGINT NOT NULL,
        last_login BIGINT NOT NULL,
        handle_name TEXT,
        handle_key TEXT,
        handle_number INTEGER,
        renamed BIGINT
    );
    -- Handles are unique case-insensitively, per number.
    CREATE UNIQUE INDEX handles ON accounts (handle_key, handle_number);
    CREATE TABLE friends (
        low BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
        high BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
        requester BIGINT NOT NULL,
        accepted BOOLEAN NOT NULL DEFAULT FALSE,
        created BIGINT NOT NULL,
        PRIMARY KEY (low, high)
    );
    CREATE INDEX friends_high ON friends (high);
    CREATE TABLE blocks (
        blocker BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
        blocked BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
        PRIMARY KEY (blocker, blocked)
    );
    CREATE INDEX blocks_blocked ON blocks (blocked);
    ",
)];

/// Taken by migrations so two doors starting at once take turns.
const MIGRATION_LOCK: i64 = 0x6172_656e_615f_6964; // "arena_id"

const ACCOUNT_COLUMNS: &str = "id, generation, handle_name, handle_number, renamed";

fn account_of(row: &Row) -> Account {
    let name: Option<String> = row.get(2);
    let number: Option<i32> = row.get(3);
    Account {
        id: row.get::<_, i64>(0) as u64,
        generation: row.get::<_, i32>(1) as u32,
        handle: name
            .zip(number)
            .and_then(|(name, number)| Handle::new(&name, u16::try_from(number).ok()?)),
        renamed: row.get::<_, Option<i64>>(4).map(|at| at as u64),
    }
}

/// WorkOS user ids longer than this are refused (theirs are ~31 bytes).
pub const MAX_SUBJECT: usize = 128;

/// Our ids are `u64`; Postgres stores them as `BIGINT`, bit for bit.
pub(super) fn db(id: u64) -> i64 {
    id as i64
}

pub struct Store {
    pool: Pool,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Store(..)")
    }
}

impl Store {
    /// Connects to the database and brings its schema up to date.
    pub async fn connect(database: &Database) -> StoreResult<Self> {
        let mut config: tokio_postgres::Config = database
            .url
            .parse()
            .map_err(|_| StoreError("ARENA_DATABASE_URL is not a postgres:// URL".into()))?;
        if let Some(schema) = &database.schema {
            if schema.is_empty()
                || !schema
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(StoreError("schema names are [a-z0-9_]".into()));
            }
            config.options(format!("-c search_path={schema}"));
        }
        config.application_name("arena-door");
        config.connect_timeout(Duration::from_secs(10));
        let manager = Manager::from_config(
            config,
            NoTls,
            ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
            },
        );
        let pool = Pool::builder(manager)
            .max_size(8)
            .wait_timeout(Some(Duration::from_secs(10)))
            .runtime(deadpool_postgres::Runtime::Tokio1)
            .build()
            .map_err(|error| StoreError(format!("database pool: {error}")))?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> StoreResult<()> {
        let mut client = self.pool.get().await?;
        let transaction = client.transaction().await?;
        transaction
            .execute("SELECT pg_advisory_xact_lock($1)", &[&MIGRATION_LOCK])
            .await?;
        transaction
            .batch_execute(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied BIGINT NOT NULL
                )",
            )
            .await?;
        let applied: HashSet<i32> = transaction
            .query("SELECT version FROM schema_migrations", &[])
            .await?
            .iter()
            .map(|row| row.get(0))
            .collect();
        for (version, sql) in MIGRATIONS {
            if applied.contains(version) {
                continue;
            }
            transaction.batch_execute(sql).await?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations (version, applied) VALUES ($1, $2)",
                    &[version, &(crate::door::unix_now() as i64)],
                )
                .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(super) async fn client(&self) -> StoreResult<deadpool_postgres::Client> {
        Ok(self.pool.get().await?)
    }

    /// The account for this WorkOS user, created on first login. `random`
    /// supplies new ids (retried on the rare collision).
    pub async fn login(
        &self,
        user: &str,
        now: u64,
        mut random: impl FnMut() -> u64,
    ) -> StoreResult<Account> {
        let client = self.client().await?;
        let now = now as i64;
        let existing = format!(
            "UPDATE accounts SET last_login = $2 WHERE workos_user = $1 RETURNING {ACCOUNT_COLUMNS}"
        );
        if let Some(row) = client.query_opt(&existing, &[&user, &now]).await? {
            return Ok(account_of(&row));
        }
        let insert = format!(
            "INSERT INTO accounts (id, workos_user, created, last_login) VALUES ($1, $2, $3, $3)
             ON CONFLICT DO NOTHING RETURNING {ACCOUNT_COLUMNS}"
        );
        for _ in 0..64 {
            let id = random();
            if id == 0 {
                continue;
            }
            if let Some(row) = client.query_opt(&insert, &[&db(id), &user, &now]).await? {
                return Ok(account_of(&row));
            }
            // Either the id was taken (try another) or another login of
            // this user just created the account (use it).
            if let Some(row) = client.query_opt(&existing, &[&user, &now]).await? {
                return Ok(account_of(&row));
            }
        }
        Err(StoreError("no free account id".into()))
    }

    /// The account with this id, if it still exists.
    pub async fn get(&self, id: u64) -> StoreResult<Option<Account>> {
        let row = self
            .client()
            .await?
            .query_opt(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = $1"),
                &[&db(id)],
            )
            .await?;
        Ok(row.as_ref().map(account_of))
    }

    /// The WorkOS user behind an account (to delete it there too).
    pub async fn workos_user(&self, id: u64) -> StoreResult<Option<String>> {
        let row = self
            .client()
            .await?
            .query_opt("SELECT workos_user FROM accounts WHERE id = $1", &[&db(id)])
            .await?;
        Ok(row.map(|row| row.get(0)))
    }

    /// Sets the account's handle to `name` (already checked against the
    /// naming rules) with a free number: the current one when only the
    /// letter case changes, else a random free one. Renames are limited to
    /// one per [`RENAME_EVERY`]; the first pick is free.
    pub async fn set_handle(
        &self,
        id: u64,
        name: &str,
        now: u64,
        mut random: impl FnMut() -> u64,
    ) -> StoreResult<Result<Handle, HandleError>> {
        let key = name.to_ascii_lowercase();
        let mut client = self.client().await?;
        let transaction = client.transaction().await?;
        // One writer per name at a time, so two players cannot take the
        // same number; the unique index is the backstop.
        transaction
            .execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&key])
            .await?;
        let Some(row) = transaction
            .query_opt(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = $1 FOR UPDATE"),
                &[&db(id)],
            )
            .await?
        else {
            return Ok(Err(HandleError::NoAccount));
        };
        let account = account_of(&row);
        let same_name = account
            .handle
            .is_some_and(|handle| handle.name().eq_ignore_ascii_case(name));
        if let (Some(_), Some(renamed), false) = (account.handle, account.renamed, same_name) {
            let allowed = renamed.saturating_add(RENAME_EVERY);
            if now < allowed {
                return Ok(Err(HandleError::TooSoon(allowed)));
            }
        }
        let taken: HashSet<u16> = transaction
            .query(
                "SELECT handle_number FROM accounts WHERE handle_key = $1 AND id <> $2",
                &[&key, &db(id)],
            )
            .await?
            .iter()
            .filter_map(|row| u16::try_from(row.get::<_, i32>(0)).ok())
            .collect();
        let mut number = account
            .handle
            .filter(|_| same_name)
            .map(|handle| handle.number);
        for _ in 0..32 {
            if number.is_some() {
                break;
            }
            let candidate = (random() % u64::from(MAX_HANDLE_NUMBER)) as u16 + 1;
            if !taken.contains(&candidate) {
                number = Some(candidate);
            }
        }
        // A crowded name: take the lowest free number, if any.
        let number = number.or_else(|| (1..=MAX_HANDLE_NUMBER).find(|n| !taken.contains(n)));
        let Some(number) = number else {
            return Ok(Err(HandleError::Full));
        };
        let handle = Handle::new(name, number).expect("names are checked before the store");
        // A case-only change keeps the rename clock where it was.
        let renamed = if same_name {
            account.renamed.unwrap_or(now)
        } else {
            now
        };
        transaction
            .execute(
                "UPDATE accounts SET handle_name = $1, handle_key = $2, handle_number = $3,
                 renamed = $4 WHERE id = $5",
                &[&name, &key, &i32::from(number), &(renamed as i64), &db(id)],
            )
            .await?;
        transaction.commit().await?;
        Ok(Ok(handle))
    }

    /// Removes the account's handle (an admin action for names that break
    /// the rules); the player picks again, with no rename wait.
    pub async fn clear_handle(&self, id: u64) -> StoreResult<bool> {
        let changed = self
            .client()
            .await?
            .execute(
                "UPDATE accounts SET handle_name = NULL, handle_key = NULL, handle_number = NULL,
                 renamed = NULL WHERE id = $1 AND handle_name IS NOT NULL",
                &[&db(id)],
            )
            .await?;
        Ok(changed == 1)
    }

    /// Ends every session of the account; `false` when it does not exist.
    pub async fn end_sessions(&self, id: u64) -> StoreResult<bool> {
        let changed = self
            .client()
            .await?
            .execute(
                "UPDATE accounts SET generation = generation + 1 WHERE id = $1",
                &[&db(id)],
            )
            .await?;
        Ok(changed == 1)
    }

    /// Deletes the account and everything stored about it: its handle,
    /// and (by cascade) friendships, requests and blocks, both ways.
    pub async fn delete(&self, id: u64) -> StoreResult<bool> {
        let changed = self
            .client()
            .await?
            .execute("DELETE FROM accounts WHERE id = $1", &[&db(id)])
            .await?;
        Ok(changed == 1)
    }

    /// The account holding `handle` (`Name#1234`, name compared without
    /// case), if any.
    pub async fn find_handle(&self, handle: &str) -> StoreResult<Option<u64>> {
        let Some((name, number)) = handle.split_once('#') else {
            return Ok(None);
        };
        // Exactly four digits, as handles are written.
        if number.len() != 4 || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(None);
        }
        let Ok(number) = number.parse::<u16>() else {
            return Ok(None);
        };
        if Handle::new(name, number).is_none() {
            return Ok(None);
        }
        let row = self
            .client()
            .await?
            .query_opt(
                "SELECT id FROM accounts WHERE handle_key = $1 AND handle_number = $2",
                &[&name.to_ascii_lowercase(), &i32::from(number)],
            )
            .await?;
        Ok(row.map(|row| row.get::<_, i64>(0) as u64))
    }

    /// Accounts stored (tests and metrics).
    pub async fn count(&self) -> StoreResult<u64> {
        let row = self
            .client()
            .await?
            .query_one("SELECT COUNT(*) FROM accounts", &[])
            .await?;
        Ok(row.get::<_, i64>(0) as u64)
    }
}

/// A fresh, empty schema in the test database named by
/// `ARENA_TEST_DATABASE_URL`, or `None` (after saying so) when it is not
/// set. `./scripts/check.sh` and CI always set it.
#[doc(hidden)]
pub async fn test_database() -> Option<Database> {
    let Ok(url) = std::env::var("ARENA_TEST_DATABASE_URL") else {
        eprintln!("skipped: set ARENA_TEST_DATABASE_URL to run account tests");
        return None;
    };
    let schema = format!("t{:016x}", crate::ticket::random_u64());
    let (client, connection) = tokio_postgres::connect(&url, NoTls)
        .await
        .expect("ARENA_TEST_DATABASE_URL reachable");
    tokio::spawn(connection);
    client
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .await
        .expect("test schema created");
    Some(Database {
        url,
        schema: Some(schema),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &'static [u64]) -> impl FnMut() -> u64 {
        let mut values = values.iter();
        move || *values.next().expect("enough ids")
    }

    async fn store() -> Option<Store> {
        Some(Store::connect(&test_database().await?).await.unwrap())
    }

    #[tokio::test]
    async fn one_account_per_workos_user() {
        let Some(store) = store().await else { return };
        let first = store.login("user_01A", 10, ids(&[7])).await.unwrap();
        assert_eq!((first.id, first.generation, first.handle), (7, 0, None));
        assert_eq!(store.login("user_01A", 20, ids(&[])).await.unwrap(), first);
        let other = store.login("user_01B", 30, ids(&[9])).await.unwrap();
        assert_eq!(other.id, 9);
        assert_eq!(store.count().await.unwrap(), 2);
        assert_eq!(
            store.workos_user(9).await.unwrap().as_deref(),
            Some("user_01B")
        );
    }

    #[tokio::test]
    async fn new_ids_skip_zero_and_collisions() {
        let Some(store) = store().await else { return };
        store.login("a", 0, ids(&[5])).await.unwrap();
        let second = store.login("b", 0, ids(&[0, 5, 6])).await.unwrap();
        assert_eq!(second.id, 6);
    }

    #[tokio::test]
    async fn ids_above_i64_max_round_trip() {
        let Some(store) = store().await else { return };
        let big = u64::MAX - 1;
        store.login("big", 0, move || big).await.unwrap();
        assert_eq!(store.get(big).await.unwrap().map(|a| a.id), Some(big));
    }

    #[tokio::test]
    async fn ending_sessions_and_deleting() {
        let Some(store) = store().await else { return };
        let account = store.login("sub", 0, ids(&[3])).await.unwrap();
        assert!(store.end_sessions(3).await.unwrap());
        assert_eq!(
            store.get(3).await.unwrap().map(|a| (a.id, a.generation)),
            Some((3, 1))
        );
        assert!(store.delete(account.id).await.unwrap());
        assert_eq!(store.get(3).await.unwrap(), None);
        assert!(!store.delete(3).await.unwrap());
        assert!(!store.end_sessions(3).await.unwrap());
        // Logging in again after deletion makes a new account.
        let again = store.login("sub", 0, ids(&[4])).await.unwrap();
        assert_eq!(again.id, 4);
    }

    #[tokio::test]
    async fn the_schema_survives_reconnecting_and_migrates_once() {
        let Some(database) = test_database().await else {
            return;
        };
        let id = Store::connect(&database)
            .await
            .unwrap()
            .login("x", 0, ids(&[11]))
            .await
            .unwrap()
            .id;
        let again = Store::connect(&database).await.unwrap();
        assert_eq!(again.get(id).await.unwrap().map(|a| a.id), Some(11));
        let versions = again
            .client()
            .await
            .unwrap()
            .query_one("SELECT COUNT(*) FROM schema_migrations", &[])
            .await
            .unwrap()
            .get::<_, i64>(0);
        assert_eq!(versions, MIGRATIONS.len() as i64);
    }

    #[tokio::test]
    async fn handles_are_unique_per_name_and_number_and_renamed_weekly() {
        let Some(store) = store().await else { return };
        for id in [1u64, 2, 3] {
            store.login(&id.to_string(), 0, move || id).await.unwrap();
        }
        // Numbers come from the random source, skipping taken ones; names
        // compare without case.
        let first = store
            .set_handle(1, "Rook", 100, ids(&[41]))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.to_string(), "Rook#0042");
        let second = store
            .set_handle(2, "rook", 100, ids(&[41, 41, 6]))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.to_string(), "rook#0007");
        assert_eq!(store.get(1).await.unwrap().unwrap().handle, Some(first));
        // A rename within a week is refused; a case change is not a rename.
        assert_eq!(
            store
                .set_handle(1, "Bishop", 100 + RENAME_EVERY - 1, ids(&[0]))
                .await
                .unwrap(),
            Err(HandleError::TooSoon(100 + RENAME_EVERY))
        );
        let recased = store
            .set_handle(1, "ROOK", 200, ids(&[]))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recased.to_string(), "ROOK#0042");
        let renamed = store
            .set_handle(1, "Bishop", 100 + RENAME_EVERY, ids(&[9]))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(renamed.to_string(), "Bishop#0010");
        assert_eq!(
            store.set_handle(99, "Rook", 0, ids(&[])).await.unwrap(),
            Err(HandleError::NoAccount)
        );
    }

    #[tokio::test]
    async fn a_crowded_name_falls_back_to_the_lowest_free_number() {
        let Some(store) = store().await else { return };
        store.login("a", 0, ids(&[1])).await.unwrap();
        store.login("b", 0, ids(&[2])).await.unwrap();
        store
            .set_handle(1, "Rook", 0, ids(&[0]))
            .await
            .unwrap()
            .unwrap();
        // The random source keeps naming the taken number 1.
        let second = store.set_handle(2, "Rook", 0, || 0).await.unwrap().unwrap();
        assert_eq!(second.number, 2);
    }

    #[tokio::test]
    async fn racing_players_never_share_a_handle() {
        let Some(store) = store().await else { return };
        let store = std::sync::Arc::new(store);
        for id in 1..=8u64 {
            store.login(&format!("r{id}"), 0, move || id).await.unwrap();
        }
        // Every random pick names number 5: one wins it, the rest fall back.
        let picks = (1..=8u64).map(|id| {
            let store = store.clone();
            tokio::spawn(async move { store.set_handle(id, "Rook", 0, || 4).await })
        });
        let mut numbers = HashSet::new();
        for pick in picks {
            let handle = pick.await.unwrap().unwrap().unwrap();
            assert!(numbers.insert(handle.number), "{handle} given twice");
        }
        assert!(numbers.contains(&5));
    }
}
