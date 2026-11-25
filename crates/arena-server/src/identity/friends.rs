//! Friends (social-identity step 4): requests, friendships and blocks,
//! stored beside the accounts. Players find each other by `Name#1234`; the
//! store never reveals whether someone blocked you.

use rusqlite::{OptionalExtension, Transaction, params};

use super::store::Store;

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS friends (
    low INTEGER NOT NULL,
    high INTEGER NOT NULL,
    requester INTEGER NOT NULL,
    accepted INTEGER NOT NULL DEFAULT 0,
    created INTEGER NOT NULL,
    PRIMARY KEY (low, high)
);
CREATE TABLE IF NOT EXISTS blocks (
    blocker INTEGER NOT NULL,
    blocked INTEGER NOT NULL,
    PRIMARY KEY (blocker, blocked)
);
";

/// Most requests one account may have waiting for answers.
pub const MAX_OUTGOING: u64 = 50;
/// Most friends an account may have.
pub const MAX_FRIENDS: u64 = 200;

/// Another player as one account sees them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Friend,
    /// They asked; you may accept or decline.
    Incoming,
    /// You asked; waiting for them.
    Outgoing,
}

impl Relation {
    pub fn id(self) -> &'static str {
        match self {
            Relation::Friend => "friend",
            Relation::Incoming => "incoming",
            Relation::Outgoing => "outgoing",
        }
    }
}

/// Why a friend action was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FriendError {
    /// No such handle, or that player blocked you (never told apart).
    NoSuchPlayer,
    /// Asking yourself.
    Yourself,
    AlreadyFriends,
    AlreadyAsked,
    /// Too many waiting requests or friends.
    TooMany,
    /// Nothing to accept, decline or remove.
    NothingThere,
}

impl FriendError {
    pub fn code(self) -> &'static str {
        match self {
            FriendError::NoSuchPlayer => "no_such_player",
            FriendError::Yourself => "yourself",
            FriendError::AlreadyFriends => "already_friends",
            FriendError::AlreadyAsked => "already_asked",
            FriendError::TooMany => "too_many",
            FriendError::NothingThere => "nothing_there",
        }
    }
}

fn pair(a: u64, b: u64) -> (i64, i64) {
    (a.min(b) as i64, a.max(b) as i64)
}

/// Removes every friendship, request and block involving `id`.
pub(super) fn forget(transaction: &Transaction<'_>, id: u64) -> rusqlite::Result<()> {
    let id = id as i64;
    transaction.execute(
        "DELETE FROM friends WHERE low = ?1 OR high = ?1",
        params![id],
    )?;
    transaction.execute(
        "DELETE FROM blocks WHERE blocker = ?1 OR blocked = ?1",
        params![id],
    )?;
    Ok(())
}

/// An edge between two accounts: who asked, and whether it was accepted.
fn edge(transaction: &Transaction<'_>, a: u64, b: u64) -> rusqlite::Result<Option<(u64, bool)>> {
    let (low, high) = pair(a, b);
    transaction
        .query_row(
            "SELECT requester, accepted FROM friends WHERE low = ?1 AND high = ?2",
            params![low, high],
            |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, bool>(1)?)),
        )
        .optional()
}

fn blocked_either_way(transaction: &Transaction<'_>, a: u64, b: u64) -> rusqlite::Result<bool> {
    transaction
        .query_row(
            "SELECT 1 FROM blocks WHERE (blocker = ?1 AND blocked = ?2) OR (blocker = ?2 AND blocked = ?1)",
            params![a as i64, b as i64],
            |_| Ok(()),
        )
        .optional()
        .map(|found| found.is_some())
}

fn count(transaction: &Transaction<'_>, sql: &str, id: u64) -> rusqlite::Result<u64> {
    transaction
        .query_row(sql, params![id as i64], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
}

impl Store {
    /// `from` asks `to` to be friends. If `to` already asked `from`, they
    /// become friends at once.
    pub fn request_friend(
        &self,
        from: u64,
        to: u64,
        now: u64,
    ) -> rusqlite::Result<Result<Relation, FriendError>> {
        if from == to {
            return Ok(Err(FriendError::Yourself));
        }
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let exists = transaction
            .query_row(
                "SELECT 1 FROM accounts WHERE id = ?1",
                params![to as i64],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists || blocked_either_way(&transaction, from, to)? {
            return Ok(Err(FriendError::NoSuchPlayer));
        }
        let result = match edge(&transaction, from, to)? {
            Some((_, true)) => Err(FriendError::AlreadyFriends),
            Some((requester, false)) if requester == from => Err(FriendError::AlreadyAsked),
            Some((_, false)) => {
                if count(&transaction, FRIENDS_OF, from)? >= MAX_FRIENDS
                    || count(&transaction, FRIENDS_OF, to)? >= MAX_FRIENDS
                {
                    return Ok(Err(FriendError::TooMany));
                }
                let (low, high) = pair(from, to);
                transaction.execute(
                    "UPDATE friends SET accepted = 1 WHERE low = ?1 AND high = ?2",
                    params![low, high],
                )?;
                Ok(Relation::Friend)
            }
            None => {
                if count(&transaction, OUTGOING_OF, from)? >= MAX_OUTGOING
                    || count(&transaction, FRIENDS_OF, from)? >= MAX_FRIENDS
                {
                    return Ok(Err(FriendError::TooMany));
                }
                let (low, high) = pair(from, to);
                transaction.execute(
                    "INSERT INTO friends (low, high, requester, created) VALUES (?1, ?2, ?3, ?4)",
                    params![low, high, from as i64, now as i64],
                )?;
                Ok(Relation::Outgoing)
            }
        };
        transaction.commit()?;
        Ok(result)
    }

    /// `me` accepts `from`'s request.
    pub fn accept_friend(&self, me: u64, from: u64) -> rusqlite::Result<Result<(), FriendError>> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let result = match edge(&transaction, me, from)? {
            Some((requester, false)) if requester == from => {
                if count(&transaction, FRIENDS_OF, me)? >= MAX_FRIENDS
                    || count(&transaction, FRIENDS_OF, from)? >= MAX_FRIENDS
                {
                    return Ok(Err(FriendError::TooMany));
                }
                let (low, high) = pair(me, from);
                transaction.execute(
                    "UPDATE friends SET accepted = 1 WHERE low = ?1 AND high = ?2",
                    params![low, high],
                )?;
                Ok(())
            }
            Some((_, true)) => Err(FriendError::AlreadyFriends),
            _ => Err(FriendError::NothingThere),
        };
        transaction.commit()?;
        Ok(result)
    }

    /// Ends whatever is between `me` and `other`: declines their request,
    /// withdraws mine, or ends a friendship.
    pub fn remove_friend(&self, me: u64, other: u64) -> rusqlite::Result<Result<(), FriendError>> {
        let (low, high) = pair(me, other);
        let changed = self.connection().execute(
            "DELETE FROM friends WHERE low = ?1 AND high = ?2",
            params![low, high],
        )?;
        Ok(if changed == 1 {
            Ok(())
        } else {
            Err(FriendError::NothingThere)
        })
    }

    /// `me` blocks `other`: ends anything between them and refuses their
    /// requests and invites until unblocked.
    pub fn block(&self, me: u64, other: u64) -> rusqlite::Result<Result<(), FriendError>> {
        if me == other {
            return Ok(Err(FriendError::Yourself));
        }
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let (low, high) = pair(me, other);
        transaction.execute(
            "DELETE FROM friends WHERE low = ?1 AND high = ?2",
            params![low, high],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO blocks (blocker, blocked) VALUES (?1, ?2)",
            params![me as i64, other as i64],
        )?;
        transaction.commit()?;
        Ok(Ok(()))
    }

    pub fn unblock(&self, me: u64, other: u64) -> rusqlite::Result<Result<(), FriendError>> {
        let changed = self.connection().execute(
            "DELETE FROM blocks WHERE blocker = ?1 AND blocked = ?2",
            params![me as i64, other as i64],
        )?;
        Ok(if changed == 1 {
            Ok(())
        } else {
            Err(FriendError::NothingThere)
        })
    }

    /// Whether `a` and `b` are friends (and neither blocked the other).
    pub fn are_friends(&self, a: u64, b: u64) -> rusqlite::Result<bool> {
        let (low, high) = pair(a, b);
        self.connection()
            .query_row(
                "SELECT 1 FROM friends WHERE low = ?1 AND high = ?2 AND accepted = 1",
                params![low, high],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
    }

    /// Everyone `me` has a relation with, with their handle (players
    /// without one are listed as `None`), and the handles `me` blocked.
    pub fn friends_of(&self, me: u64) -> rusqlite::Result<Friends> {
        let connection = self.connection();
        let me_i = me as i64;
        let mut statement = connection.prepare(
            "SELECT f.requester, f.accepted, a.id, a.handle_name, a.handle_number
             FROM friends f JOIN accounts a
               ON a.id = CASE WHEN f.low = ?1 THEN f.high ELSE f.low END
             WHERE f.low = ?1 OR f.high = ?1
             ORDER BY f.accepted DESC, a.handle_key, a.handle_number",
        )?;
        let people = statement
            .query_map(params![me_i], |row| {
                let requester = row.get::<_, i64>(0)? as u64;
                let accepted: bool = row.get(1)?;
                let relation = match (accepted, requester == me) {
                    (true, _) => Relation::Friend,
                    (false, true) => Relation::Outgoing,
                    (false, false) => Relation::Incoming,
                };
                Ok(Person {
                    account: row.get::<_, i64>(2)? as u64,
                    handle: handle_of(row.get(3)?, row.get(4)?),
                    relation,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut statement = connection.prepare(
            "SELECT a.handle_name, a.handle_number FROM blocks b JOIN accounts a ON a.id = b.blocked
             WHERE b.blocker = ?1",
        )?;
        let blocked = statement
            .query_map(params![me_i], |row| Ok(handle_of(row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(Friends { people, blocked })
    }
}

const FRIENDS_OF: &str =
    "SELECT COUNT(*) FROM friends WHERE (low = ?1 OR high = ?1) AND accepted = 1";
const OUTGOING_OF: &str = "SELECT COUNT(*) FROM friends WHERE requester = ?1 AND accepted = 0";

fn handle_of(name: Option<String>, number: Option<u16>) -> Option<String> {
    name.zip(number)
        .and_then(|(name, number)| arena_protocol::Handle::new(&name, number))
        .map(|handle| handle.to_string())
}

/// Someone `me` has a relation with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub account: u64,
    pub handle: Option<String>,
    pub relation: Relation,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Friends {
    pub people: Vec<Person>,
    /// Handles `me` blocked.
    pub blocked: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::super::store::ProviderId;
    use super::*;

    fn store_with(accounts: &[(u64, &str)]) -> Store {
        let store = Store::in_memory().unwrap();
        for &(id, name) in accounts {
            store
                .login(ProviderId::Github, &id.to_string(), 0, move || id)
                .unwrap();
            store.set_handle(id, name, 0, move || id).unwrap().unwrap();
        }
        store
    }

    #[test]
    fn requests_become_friendships_both_ways() {
        let store = store_with(&[(1, "Rook"), (2, "Ash"), (3, "Bishop")]);
        assert_eq!(
            store.request_friend(1, 2, 0).unwrap(),
            Ok(Relation::Outgoing)
        );
        assert_eq!(
            store.request_friend(1, 2, 0).unwrap(),
            Err(FriendError::AlreadyAsked)
        );
        assert_eq!(
            store.request_friend(1, 1, 0).unwrap(),
            Err(FriendError::Yourself)
        );
        assert_eq!(
            store.request_friend(1, 99, 0).unwrap(),
            Err(FriendError::NoSuchPlayer)
        );
        let ash = store.friends_of(2).unwrap();
        assert_eq!(ash.people[0].relation, Relation::Incoming);
        assert_eq!(ash.people[0].handle.as_deref(), Some("Rook#0002"));
        assert_eq!(store.accept_friend(2, 1).unwrap(), Ok(()));
        assert!(store.are_friends(1, 2).unwrap());
        assert_eq!(
            store.request_friend(2, 1, 0).unwrap(),
            Err(FriendError::AlreadyFriends)
        );
        // Asking someone who already asked you accepts at once.
        assert_eq!(
            store.request_friend(3, 1, 0).unwrap(),
            Ok(Relation::Outgoing)
        );
        assert_eq!(store.request_friend(1, 3, 0).unwrap(), Ok(Relation::Friend));
        assert_eq!(store.friends_of(1).unwrap().people.len(), 2);
        // Declining, withdrawing and unfriending all end the edge.
        assert_eq!(store.remove_friend(1, 2).unwrap(), Ok(()));
        assert_eq!(
            store.remove_friend(1, 2).unwrap(),
            Err(FriendError::NothingThere)
        );
        assert!(!store.are_friends(1, 2).unwrap());
        assert_eq!(
            store.accept_friend(2, 1).unwrap(),
            Err(FriendError::NothingThere)
        );
    }

    #[test]
    fn blocks_hide_you_and_end_friendships() {
        let store = store_with(&[(1, "Rook"), (2, "Ash")]);
        store.request_friend(1, 2, 0).unwrap().unwrap();
        store.accept_friend(2, 1).unwrap().unwrap();
        assert_eq!(store.block(2, 1).unwrap(), Ok(()));
        assert!(!store.are_friends(1, 2).unwrap());
        // The blocked player cannot tell a block from a missing player, and
        // the blocker cannot ask either until they unblock.
        assert_eq!(
            store.request_friend(1, 2, 0).unwrap(),
            Err(FriendError::NoSuchPlayer)
        );
        assert_eq!(
            store.request_friend(2, 1, 0).unwrap(),
            Err(FriendError::NoSuchPlayer)
        );
        assert_eq!(store.friends_of(2).unwrap().blocked, ["Rook#0002"]);
        assert_eq!(store.unblock(2, 1).unwrap(), Ok(()));
        assert_eq!(
            store.request_friend(1, 2, 0).unwrap(),
            Ok(Relation::Outgoing)
        );
    }

    #[test]
    fn requests_are_capped_and_deletion_forgets_everything() {
        let ids: Vec<(u64, String)> = (1..=MAX_OUTGOING + 2)
            .map(|id| (id, format!("Player{id}")))
            .collect();
        let named: Vec<(u64, &str)> = ids.iter().map(|(id, name)| (*id, name.as_str())).collect();
        let store = store_with(&named);
        for to in 2..=MAX_OUTGOING + 1 {
            assert_eq!(
                store.request_friend(1, to, 0).unwrap(),
                Ok(Relation::Outgoing)
            );
        }
        assert_eq!(
            store.request_friend(1, MAX_OUTGOING + 2, 0).unwrap(),
            Err(FriendError::TooMany)
        );
        store.block(2, 1).unwrap().unwrap();
        assert!(store.delete(1).unwrap());
        assert!(store.friends_of(3).unwrap().people.is_empty());
        assert!(store.friends_of(2).unwrap().blocked.is_empty());
    }

    #[test]
    fn handles_find_their_accounts_without_case() {
        let store = store_with(&[(7, "Rook")]);
        assert_eq!(store.find_handle("rook#0008").unwrap(), Some(7));
        for missing in [
            "Rook#0009",
            "Rook",
            "Rook#8",
            "Rook#00008",
            "Ro ok#0008",
            "#0008",
        ] {
            assert_eq!(store.find_handle(missing).unwrap(), None, "{missing}");
        }
    }
}
