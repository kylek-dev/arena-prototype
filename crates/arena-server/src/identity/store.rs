//! The account store (ADR 0055): one SQLite file with the minimum about each
//! account. Calls block; the door runs them off the async workers.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};

/// Providers a player can log in with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderId {
    Discord,
    Google,
    Github,
}

impl ProviderId {
    pub const ALL: [ProviderId; 3] = [ProviderId::Discord, ProviderId::Google, ProviderId::Github];

    pub fn id(self) -> &'static str {
        match self {
            ProviderId::Discord => "discord",
            ProviderId::Google => "google",
            ProviderId::Github => "github",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|provider| provider.id() == id)
    }
}

/// An account as sessions see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    /// Our random id, never 0.
    pub id: u64,
    /// Raised to end every session at once (log out everywhere, bans).
    pub generation: u32,
}

#[derive(Debug)]
pub struct Store(Mutex<Connection>);

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    subject TEXT NOT NULL,
    generation INTEGER NOT NULL DEFAULT 0,
    created INTEGER NOT NULL,
    last_login INTEGER NOT NULL,
    UNIQUE (provider, subject)
);
";

/// Subjects longer than this are refused (providers' ids are short).
pub const MAX_SUBJECT: usize = 128;

impl Store {
    /// Opens (creating if needed) the store at `path`.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        // WAL keeps readers and the writer apart; back up with the online
        // backup API, never by copying the file.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Self::with(connection)
    }

    /// A store that lives only in memory (tests, and a door without a path).
    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(connection: Connection) -> rusqlite::Result<Self> {
        connection.execute_batch(SCHEMA)?;
        Ok(Self(Mutex::new(connection)))
    }

    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// The account for this provider identity, created on first login.
    /// `random` supplies new ids (retried on the rare collision).
    pub fn login(
        &self,
        provider: ProviderId,
        subject: &str,
        now: u64,
        mut random: impl FnMut() -> u64,
    ) -> rusqlite::Result<Account> {
        let connection = self.connection();
        let existing = connection
            .query_row(
                "SELECT id, generation FROM accounts WHERE provider = ?1 AND subject = ?2",
                params![provider.id(), subject],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, u32>(1)?)),
            )
            .optional()?;
        if let Some((id, generation)) = existing {
            connection.execute(
                "UPDATE accounts SET last_login = ?1 WHERE id = ?2",
                params![now as i64, id],
            )?;
            return Ok(Account {
                id: id as u64,
                generation,
            });
        }
        loop {
            let id = random();
            if id == 0 {
                continue;
            }
            // Only an id collision retries; a subject conflict (another
            // writer) is an error rather than a loop.
            let inserted = connection.execute(
                "INSERT INTO accounts (id, provider, subject, created, last_login)
                 VALUES (?1, ?2, ?3, ?4, ?4) ON CONFLICT(id) DO NOTHING",
                params![id as i64, provider.id(), subject, now as i64],
            )?;
            if inserted == 1 {
                return Ok(Account { id, generation: 0 });
            }
        }
    }

    /// The account with this id, if it still exists.
    pub fn get(&self, id: u64) -> rusqlite::Result<Option<Account>> {
        self.connection()
            .query_row(
                "SELECT generation FROM accounts WHERE id = ?1",
                params![id as i64],
                |row| row.get::<_, u32>(0),
            )
            .optional()
            .map(|found| found.map(|generation| Account { id, generation }))
    }

    /// Ends every session of the account; `false` when it does not exist.
    pub fn end_sessions(&self, id: u64) -> rusqlite::Result<bool> {
        let changed = self.connection().execute(
            "UPDATE accounts SET generation = generation + 1 WHERE id = ?1",
            params![id as i64],
        )?;
        Ok(changed == 1)
    }

    /// Deletes the account and everything stored about it.
    pub fn delete(&self, id: u64) -> rusqlite::Result<bool> {
        let changed = self
            .connection()
            .execute("DELETE FROM accounts WHERE id = ?1", params![id as i64])?;
        Ok(changed == 1)
    }

    /// Accounts stored (tests and metrics).
    pub fn count(&self) -> rusqlite::Result<u64> {
        self.connection()
            .query_row("SELECT COUNT(*) FROM accounts", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &'static [u64]) -> impl FnMut() -> u64 {
        let mut values = values.iter();
        move || *values.next().expect("enough ids")
    }

    #[test]
    fn one_account_per_provider_identity() {
        let store = Store::in_memory().unwrap();
        let first = store
            .login(ProviderId::Discord, "80351110224678912", 10, ids(&[7]))
            .unwrap();
        assert_eq!(
            first,
            Account {
                id: 7,
                generation: 0
            }
        );
        // The same identity logs into the same account; another provider's
        // identical subject is another person.
        assert_eq!(
            store
                .login(ProviderId::Discord, "80351110224678912", 20, ids(&[]))
                .unwrap(),
            first
        );
        let other = store
            .login(ProviderId::Github, "80351110224678912", 30, ids(&[9]))
            .unwrap();
        assert_eq!(other.id, 9);
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn new_ids_skip_zero_and_collisions() {
        let store = Store::in_memory().unwrap();
        store.login(ProviderId::Google, "a", 0, ids(&[5])).unwrap();
        let second = store
            .login(ProviderId::Google, "b", 0, ids(&[0, 5, 6]))
            .unwrap();
        assert_eq!(second.id, 6);
    }

    #[test]
    fn ending_sessions_and_deleting() {
        let store = Store::in_memory().unwrap();
        let account = store
            .login(ProviderId::Google, "sub", 0, ids(&[3]))
            .unwrap();
        assert!(store.end_sessions(3).unwrap());
        assert_eq!(
            store.get(3).unwrap(),
            Some(Account {
                id: 3,
                generation: 1
            })
        );
        assert!(store.delete(account.id).unwrap());
        assert_eq!(store.get(3).unwrap(), None);
        assert!(!store.delete(3).unwrap());
        assert!(!store.end_sessions(3).unwrap());
        // Logging in again after deletion makes a new account.
        let again = store
            .login(ProviderId::Google, "sub", 0, ids(&[4]))
            .unwrap();
        assert_eq!(again.id, 4);
    }

    #[test]
    fn a_file_store_survives_reopening() {
        let dir =
            std::env::temp_dir().join(format!("arena-identity-{}", crate::ticket::random_u64()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.sqlite3");
        let id = Store::open(&path)
            .unwrap()
            .login(ProviderId::Discord, "x", 0, ids(&[11]))
            .unwrap()
            .id;
        assert_eq!(
            Store::open(&path).unwrap().get(id).unwrap().map(|a| a.id),
            Some(11)
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
