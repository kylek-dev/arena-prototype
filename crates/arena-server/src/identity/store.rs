//! The account store (ADR 0055): one SQLite file with the minimum about each
//! account. Calls block; the door runs them off the async workers.

use std::path::Path;
use std::sync::Mutex;

use arena_protocol::{Handle, MAX_HANDLE_NUMBER};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

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
    handle_name TEXT,
    handle_key TEXT,
    handle_number INTEGER,
    renamed INTEGER,
    UNIQUE (provider, subject)
);
";

/// Added by social-identity step 3, for stores created before it.
const HANDLE_COLUMNS: [&str; 4] = [
    "handle_name TEXT",
    "handle_key TEXT",
    "handle_number INTEGER",
    "renamed INTEGER",
];

/// Handles are unique case-insensitively, per number.
const HANDLE_INDEX: &str =
    "CREATE UNIQUE INDEX IF NOT EXISTS handles ON accounts (handle_key, handle_number)";

const ACCOUNT_COLUMNS: &str = "id, generation, handle_name, handle_number, renamed";

fn account_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let name: Option<String> = row.get(2)?;
    let number: Option<u16> = row.get(3)?;
    Ok(Account {
        id: row.get::<_, i64>(0)? as u64,
        generation: row.get(1)?,
        handle: name
            .zip(number)
            .and_then(|(name, number)| Handle::new(&name, number)),
        renamed: row.get::<_, Option<i64>>(4)?.map(|at| at as u64),
    })
}

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
        let existing: Vec<String> = connection
            .prepare("SELECT name FROM pragma_table_info('accounts')")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for column in HANDLE_COLUMNS {
            let name = column.split(' ').next().expect("named");
            if !existing.iter().any(|have| have == name) {
                connection.execute(&format!("ALTER TABLE accounts ADD COLUMN {column}"), [])?;
            }
        }
        connection.execute(HANDLE_INDEX, [])?;
        connection.execute_batch(super::friends::SCHEMA)?;
        Ok(Self(Mutex::new(connection)))
    }

    pub(super) fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
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
                &format!(
                    "SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE provider = ?1 AND subject = ?2"
                ),
                params![provider.id(), subject],
                account_of,
            )
            .optional()?;
        if let Some(account) = existing {
            connection.execute(
                "UPDATE accounts SET last_login = ?1 WHERE id = ?2",
                params![now as i64, account.id as i64],
            )?;
            return Ok(account);
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
                return Ok(Account {
                    id,
                    generation: 0,
                    handle: None,
                    renamed: None,
                });
            }
        }
    }

    /// The account with this id, if it still exists.
    pub fn get(&self, id: u64) -> rusqlite::Result<Option<Account>> {
        self.connection()
            .query_row(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = ?1"),
                params![id as i64],
                account_of,
            )
            .optional()
    }

    /// Sets the account's handle to `name` (already checked against the
    /// naming rules) with a free number: the current one when only the
    /// letter case changes, else a random free one. Renames are limited to
    /// one per [`RENAME_EVERY`]; the first pick is free.
    pub fn set_handle(
        &self,
        id: u64,
        name: &str,
        now: u64,
        mut random: impl FnMut() -> u64,
    ) -> rusqlite::Result<Result<Handle, HandleError>> {
        let mut connection = self.connection();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(account) = transaction
            .query_row(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = ?1"),
                params![id as i64],
                account_of,
            )
            .optional()?
        else {
            return Ok(Err(HandleError::NoAccount));
        };
        let key = name.to_ascii_lowercase();
        let same_name = account
            .handle
            .is_some_and(|handle| handle.name().eq_ignore_ascii_case(name));
        if let (Some(_), Some(renamed), false) = (account.handle, account.renamed, same_name) {
            let allowed = renamed.saturating_add(RENAME_EVERY);
            if now < allowed {
                return Ok(Err(HandleError::TooSoon(allowed)));
            }
        }
        let taken = |number: u16| -> rusqlite::Result<bool> {
            transaction
                .query_row(
                    "SELECT 1 FROM accounts WHERE handle_key = ?1 AND handle_number = ?2 AND id != ?3",
                    params![key, number, id as i64],
                    |_| Ok(()),
                )
                .optional()
                .map(|found| found.is_some())
        };
        let mut number = account
            .handle
            .filter(|_| same_name)
            .map(|handle| handle.number);
        for _ in 0..32 {
            if number.is_some() {
                break;
            }
            let candidate = (random() % u64::from(MAX_HANDLE_NUMBER)) as u16 + 1;
            if !taken(candidate)? {
                number = Some(candidate);
            }
        }
        if number.is_none() {
            // A crowded name: take the lowest free number, if any.
            for candidate in 1..=MAX_HANDLE_NUMBER {
                if !taken(candidate)? {
                    number = Some(candidate);
                    break;
                }
            }
        }
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
        transaction.execute(
            "UPDATE accounts SET handle_name = ?1, handle_key = ?2, handle_number = ?3, renamed = ?4
             WHERE id = ?5",
            params![name, key, number, renamed as i64, id as i64],
        )?;
        transaction.commit()?;
        Ok(Ok(handle))
    }

    /// Removes the account's handle (an admin action for names that break
    /// the rules); the player picks again, with no rename wait.
    pub fn clear_handle(&self, id: u64) -> rusqlite::Result<bool> {
        let changed = self.connection().execute(
            "UPDATE accounts SET handle_name = NULL, handle_key = NULL, handle_number = NULL,
             renamed = NULL WHERE id = ?1 AND handle_name IS NOT NULL",
            params![id as i64],
        )?;
        Ok(changed == 1)
    }

    /// Ends every session of the account; `false` when it does not exist.
    pub fn end_sessions(&self, id: u64) -> rusqlite::Result<bool> {
        let changed = self.connection().execute(
            "UPDATE accounts SET generation = generation + 1 WHERE id = ?1",
            params![id as i64],
        )?;
        Ok(changed == 1)
    }

    /// Deletes the account and everything stored about it: its handle,
    /// friendships, requests and blocks, both ways.
    pub fn delete(&self, id: u64) -> rusqlite::Result<bool> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        super::friends::forget(&transaction, id)?;
        let changed =
            transaction.execute("DELETE FROM accounts WHERE id = ?1", params![id as i64])?;
        transaction.commit()?;
        Ok(changed == 1)
    }

    /// The account holding `handle` (`Name#1234`, name compared without
    /// case), if any.
    pub fn find_handle(&self, handle: &str) -> rusqlite::Result<Option<u64>> {
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
        self.connection()
            .query_row(
                "SELECT id FROM accounts WHERE handle_key = ?1 AND handle_number = ?2",
                params![name.to_ascii_lowercase(), number],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|id| id.map(|id| id as u64))
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
        assert_eq!((first.id, first.generation, first.handle), (7, 0, None));
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
            store.get(3).unwrap().map(|a| (a.id, a.generation)),
            Some((3, 1))
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

    #[test]
    fn handles_are_unique_per_name_and_number_and_renamed_weekly() {
        let store = Store::in_memory().unwrap();
        for id in [1u64, 2, 3] {
            store
                .login(ProviderId::Github, &id.to_string(), 0, move || id)
                .unwrap();
        }
        // Numbers come from the random source, skipping taken ones; names
        // compare without case.
        let first = store
            .set_handle(1, "Rook", 100, ids(&[41]))
            .unwrap()
            .unwrap();
        assert_eq!(first.to_string(), "Rook#0042");
        let second = store
            .set_handle(2, "rook", 100, ids(&[41, 41, 6]))
            .unwrap()
            .unwrap();
        assert_eq!(second.to_string(), "rook#0007");
        assert_eq!(store.get(1).unwrap().unwrap().handle, Some(first));
        // A rename within a week is refused; a case change is not a rename.
        assert_eq!(
            store
                .set_handle(1, "Bishop", 100 + RENAME_EVERY - 1, ids(&[0]))
                .unwrap(),
            Err(HandleError::TooSoon(100 + RENAME_EVERY))
        );
        let recased = store.set_handle(1, "ROOK", 200, ids(&[])).unwrap().unwrap();
        assert_eq!(recased.to_string(), "ROOK#0042");
        let renamed = store
            .set_handle(1, "Bishop", 100 + RENAME_EVERY, ids(&[9]))
            .unwrap()
            .unwrap();
        assert_eq!(renamed.to_string(), "Bishop#0010");
        assert_eq!(
            store.set_handle(99, "Rook", 0, ids(&[])).unwrap(),
            Err(HandleError::NoAccount)
        );
    }

    #[test]
    fn a_crowded_name_falls_back_to_the_lowest_free_number() {
        let store = Store::in_memory().unwrap();
        store.login(ProviderId::Github, "a", 0, ids(&[1])).unwrap();
        store.login(ProviderId::Github, "b", 0, ids(&[2])).unwrap();
        store.set_handle(1, "Rook", 0, ids(&[0])).unwrap().unwrap();
        // The random source keeps naming the taken number 1.
        let second = store.set_handle(2, "Rook", 0, || 0).unwrap().unwrap();
        assert_eq!(second.number, 2);
    }

    #[test]
    fn stores_from_before_handles_gain_the_columns() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE accounts (id INTEGER PRIMARY KEY, provider TEXT NOT NULL,
                 subject TEXT NOT NULL, generation INTEGER NOT NULL DEFAULT 0,
                 created INTEGER NOT NULL, last_login INTEGER NOT NULL,
                 UNIQUE (provider, subject));
                 INSERT INTO accounts (id, provider, subject, created, last_login)
                 VALUES (5, 'github', 'x', 0, 0);",
            )
            .unwrap();
        let store = Store::with(connection).unwrap();
        assert_eq!(store.get(5).unwrap().unwrap().handle, None);
        assert!(store.set_handle(5, "Rook", 0, ids(&[0])).unwrap().is_ok());
    }
}
