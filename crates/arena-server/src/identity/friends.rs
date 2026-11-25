//! Friends (social-identity step 4): requests, friendships and blocks,
//! stored beside the accounts. Players find each other by `Name#1234`; the
//! store never reveals whether someone blocked you.

use tokio_postgres::Transaction;

use super::store::{Store, StoreResult, db};

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
    (db(a.min(b)), db(a.max(b)))
}

/// Locks both accounts' rows for the rest of the transaction, so their
/// counts cannot change underneath a check; whether `other` exists.
async fn lock_pair(transaction: &Transaction<'_>, me: u64, other: u64) -> StoreResult<bool> {
    let ids = vec![db(me.min(other)), db(me.max(other))];
    let rows = transaction
        .query(
            "SELECT id FROM accounts WHERE id = ANY($1) ORDER BY id FOR UPDATE",
            &[&ids],
        )
        .await?;
    Ok(rows.iter().any(|row| row.get::<_, i64>(0) == db(other)))
}

/// An edge between two accounts: who asked, and whether it was accepted.
async fn edge(transaction: &Transaction<'_>, a: u64, b: u64) -> StoreResult<Option<(u64, bool)>> {
    let (low, high) = pair(a, b);
    let row = transaction
        .query_opt(
            "SELECT requester, accepted FROM friends WHERE low = $1 AND high = $2",
            &[&low, &high],
        )
        .await?;
    Ok(row.map(|row| (row.get::<_, i64>(0) as u64, row.get(1))))
}

async fn blocked_either_way(transaction: &Transaction<'_>, a: u64, b: u64) -> StoreResult<bool> {
    let row = transaction
        .query_opt(
            "SELECT 1 FROM blocks WHERE (blocker = $1 AND blocked = $2) OR (blocker = $2 AND blocked = $1)",
            &[&db(a), &db(b)],
        )
        .await?;
    Ok(row.is_some())
}

async fn count(transaction: &Transaction<'_>, sql: &str, id: u64) -> StoreResult<u64> {
    let row = transaction.query_one(sql, &[&db(id)]).await?;
    Ok(row.get::<_, i64>(0) as u64)
}

impl Store {
    /// `from` asks `to` to be friends. If `to` already asked `from`, they
    /// become friends at once.
    pub async fn request_friend(
        &self,
        from: u64,
        to: u64,
        now: u64,
    ) -> StoreResult<Result<Relation, FriendError>> {
        if from == to {
            return Ok(Err(FriendError::Yourself));
        }
        let mut client = self.client().await?;
        let transaction = client.transaction().await?;
        if !lock_pair(&transaction, from, to).await?
            || blocked_either_way(&transaction, from, to).await?
        {
            return Ok(Err(FriendError::NoSuchPlayer));
        }
        let (low, high) = pair(from, to);
        let result = match edge(&transaction, from, to).await? {
            Some((_, true)) => Err(FriendError::AlreadyFriends),
            Some((requester, false)) if requester == from => Err(FriendError::AlreadyAsked),
            Some((_, false)) => {
                if count(&transaction, FRIENDS_OF, from).await? >= MAX_FRIENDS
                    || count(&transaction, FRIENDS_OF, to).await? >= MAX_FRIENDS
                {
                    return Ok(Err(FriendError::TooMany));
                }
                transaction
                    .execute(
                        "UPDATE friends SET accepted = TRUE WHERE low = $1 AND high = $2",
                        &[&low, &high],
                    )
                    .await?;
                Ok(Relation::Friend)
            }
            None => {
                if count(&transaction, OUTGOING_OF, from).await? >= MAX_OUTGOING
                    || count(&transaction, FRIENDS_OF, from).await? >= MAX_FRIENDS
                {
                    return Ok(Err(FriendError::TooMany));
                }
                transaction
                    .execute(
                        "INSERT INTO friends (low, high, requester, created) VALUES ($1, $2, $3, $4)",
                        &[&low, &high, &db(from), &(now as i64)],
                    )
                    .await?;
                Ok(Relation::Outgoing)
            }
        };
        transaction.commit().await?;
        Ok(result)
    }

    /// `me` accepts `from`'s request.
    pub async fn accept_friend(&self, me: u64, from: u64) -> StoreResult<Result<(), FriendError>> {
        let mut client = self.client().await?;
        let transaction = client.transaction().await?;
        lock_pair(&transaction, me, from).await?;
        let result = match edge(&transaction, me, from).await? {
            Some((requester, false)) if requester == from => {
                if count(&transaction, FRIENDS_OF, me).await? >= MAX_FRIENDS
                    || count(&transaction, FRIENDS_OF, from).await? >= MAX_FRIENDS
                {
                    return Ok(Err(FriendError::TooMany));
                }
                let (low, high) = pair(me, from);
                transaction
                    .execute(
                        "UPDATE friends SET accepted = TRUE WHERE low = $1 AND high = $2",
                        &[&low, &high],
                    )
                    .await?;
                Ok(())
            }
            Some((_, true)) => Err(FriendError::AlreadyFriends),
            _ => Err(FriendError::NothingThere),
        };
        transaction.commit().await?;
        Ok(result)
    }

    /// Ends whatever is between `me` and `other`: declines their request,
    /// withdraws mine, or ends a friendship.
    pub async fn remove_friend(&self, me: u64, other: u64) -> StoreResult<Result<(), FriendError>> {
        let (low, high) = pair(me, other);
        let changed = self
            .client()
            .await?
            .execute(
                "DELETE FROM friends WHERE low = $1 AND high = $2",
                &[&low, &high],
            )
            .await?;
        Ok(if changed == 1 {
            Ok(())
        } else {
            Err(FriendError::NothingThere)
        })
    }

    /// `me` blocks `other`: ends anything between them and refuses their
    /// requests and invites until unblocked.
    pub async fn block(&self, me: u64, other: u64) -> StoreResult<Result<(), FriendError>> {
        if me == other {
            return Ok(Err(FriendError::Yourself));
        }
        let mut client = self.client().await?;
        let transaction = client.transaction().await?;
        if !lock_pair(&transaction, me, other).await? {
            return Ok(Err(FriendError::NoSuchPlayer));
        }
        let (low, high) = pair(me, other);
        transaction
            .execute(
                "DELETE FROM friends WHERE low = $1 AND high = $2",
                &[&low, &high],
            )
            .await?;
        transaction
            .execute(
                "INSERT INTO blocks (blocker, blocked) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                &[&db(me), &db(other)],
            )
            .await?;
        transaction.commit().await?;
        Ok(Ok(()))
    }

    pub async fn unblock(&self, me: u64, other: u64) -> StoreResult<Result<(), FriendError>> {
        let changed = self
            .client()
            .await?
            .execute(
                "DELETE FROM blocks WHERE blocker = $1 AND blocked = $2",
                &[&db(me), &db(other)],
            )
            .await?;
        Ok(if changed == 1 {
            Ok(())
        } else {
            Err(FriendError::NothingThere)
        })
    }

    /// The accounts `me` blocked.
    pub async fn blocked_by(&self, me: u64) -> StoreResult<Vec<u64>> {
        let rows = self
            .client()
            .await?
            .query("SELECT blocked FROM blocks WHERE blocker = $1", &[&db(me)])
            .await?;
        Ok(rows.iter().map(|row| row.get::<_, i64>(0) as u64).collect())
    }

    /// Whether `a` and `b` are friends (and neither blocked the other).
    pub async fn are_friends(&self, a: u64, b: u64) -> StoreResult<bool> {
        let (low, high) = pair(a, b);
        let row = self
            .client()
            .await?
            .query_opt(
                "SELECT 1 FROM friends WHERE low = $1 AND high = $2 AND accepted",
                &[&low, &high],
            )
            .await?;
        Ok(row.is_some())
    }

    /// Everyone `me` has a relation with, with their handle (players
    /// without one are listed as `None`), and the handles `me` blocked.
    pub async fn friends_of(&self, me: u64) -> StoreResult<Friends> {
        let client = self.client().await?;
        let rows = client
            .query(
                "SELECT f.requester, f.accepted, a.id, a.handle_name, a.handle_number
                 FROM friends f JOIN accounts a
                   ON a.id = CASE WHEN f.low = $1 THEN f.high ELSE f.low END
                 WHERE f.low = $1 OR f.high = $1
                 ORDER BY f.accepted DESC, a.handle_key, a.handle_number",
                &[&db(me)],
            )
            .await?;
        let people = rows
            .iter()
            .map(|row| {
                let requester = row.get::<_, i64>(0) as u64;
                let accepted: bool = row.get(1);
                let relation = match (accepted, requester == me) {
                    (true, _) => Relation::Friend,
                    (false, true) => Relation::Outgoing,
                    (false, false) => Relation::Incoming,
                };
                Person {
                    account: row.get::<_, i64>(2) as u64,
                    handle: handle_of(row.get(3), row.get(4)),
                    relation,
                }
            })
            .collect();
        let blocked = client
            .query(
                "SELECT a.handle_name, a.handle_number FROM blocks b JOIN accounts a ON a.id = b.blocked
                 WHERE b.blocker = $1 ORDER BY a.handle_key, a.handle_number",
                &[&db(me)],
            )
            .await?
            .iter()
            .filter_map(|row| handle_of(row.get(0), row.get(1)))
            .collect();
        Ok(Friends { people, blocked })
    }
}

const FRIENDS_OF: &str = "SELECT COUNT(*) FROM friends WHERE (low = $1 OR high = $1) AND accepted";
const OUTGOING_OF: &str = "SELECT COUNT(*) FROM friends WHERE requester = $1 AND NOT accepted";

fn handle_of(name: Option<String>, number: Option<i32>) -> Option<String> {
    name.zip(number)
        .and_then(|(name, number)| arena_protocol::Handle::new(&name, u16::try_from(number).ok()?))
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
    use super::super::store::test_database;
    use super::*;

    async fn store_with(accounts: &[(u64, &str)]) -> Option<Store> {
        let store = Store::connect(&test_database().await?).await.unwrap();
        for &(id, name) in accounts {
            store.login(&id.to_string(), 0, move || id).await.unwrap();
            store
                .set_handle(id, name, 0, move || id)
                .await
                .unwrap()
                .unwrap();
        }
        Some(store)
    }

    #[tokio::test]
    async fn requests_become_friendships_both_ways() {
        let Some(store) = store_with(&[(1, "Rook"), (2, "Ash"), (3, "Bishop")]).await else {
            return;
        };
        assert_eq!(
            store.request_friend(1, 2, 0).await.unwrap(),
            Ok(Relation::Outgoing)
        );
        assert_eq!(
            store.request_friend(1, 2, 0).await.unwrap(),
            Err(FriendError::AlreadyAsked)
        );
        assert_eq!(
            store.request_friend(1, 1, 0).await.unwrap(),
            Err(FriendError::Yourself)
        );
        assert_eq!(
            store.request_friend(1, 99, 0).await.unwrap(),
            Err(FriendError::NoSuchPlayer)
        );
        let ash = store.friends_of(2).await.unwrap();
        assert_eq!(ash.people[0].relation, Relation::Incoming);
        assert_eq!(ash.people[0].handle.as_deref(), Some("Rook#0002"));
        assert_eq!(store.accept_friend(2, 1).await.unwrap(), Ok(()));
        assert!(store.are_friends(1, 2).await.unwrap());
        assert_eq!(
            store.request_friend(2, 1, 0).await.unwrap(),
            Err(FriendError::AlreadyFriends)
        );
        // Asking someone who already asked you accepts at once.
        assert_eq!(
            store.request_friend(3, 1, 0).await.unwrap(),
            Ok(Relation::Outgoing)
        );
        assert_eq!(
            store.request_friend(1, 3, 0).await.unwrap(),
            Ok(Relation::Friend)
        );
        assert_eq!(store.friends_of(1).await.unwrap().people.len(), 2);
        // Declining, withdrawing and unfriending all end the edge.
        assert_eq!(store.remove_friend(1, 2).await.unwrap(), Ok(()));
        assert_eq!(
            store.remove_friend(1, 2).await.unwrap(),
            Err(FriendError::NothingThere)
        );
        assert!(!store.are_friends(1, 2).await.unwrap());
        assert_eq!(
            store.accept_friend(2, 1).await.unwrap(),
            Err(FriendError::NothingThere)
        );
    }

    #[tokio::test]
    async fn blocks_hide_you_and_end_friendships() {
        let Some(store) = store_with(&[(1, "Rook"), (2, "Ash")]).await else {
            return;
        };
        store.request_friend(1, 2, 0).await.unwrap().unwrap();
        store.accept_friend(2, 1).await.unwrap().unwrap();
        assert_eq!(store.block(2, 1).await.unwrap(), Ok(()));
        assert!(!store.are_friends(1, 2).await.unwrap());
        // The blocked player cannot tell a block from a missing player, and
        // the blocker cannot ask either until they unblock.
        assert_eq!(
            store.request_friend(1, 2, 0).await.unwrap(),
            Err(FriendError::NoSuchPlayer)
        );
        assert_eq!(
            store.request_friend(2, 1, 0).await.unwrap(),
            Err(FriendError::NoSuchPlayer)
        );
        assert_eq!(store.friends_of(2).await.unwrap().blocked, ["Rook#0002"]);
        assert_eq!(store.unblock(2, 1).await.unwrap(), Ok(()));
        assert_eq!(
            store.request_friend(1, 2, 0).await.unwrap(),
            Ok(Relation::Outgoing)
        );
    }

    #[tokio::test]
    async fn requests_are_capped_and_deletion_forgets_everything() {
        let ids: Vec<(u64, String)> = (1..=MAX_OUTGOING + 2)
            .map(|id| (id, format!("Player{id}")))
            .collect();
        let named: Vec<(u64, &str)> = ids.iter().map(|(id, name)| (*id, name.as_str())).collect();
        let Some(store) = store_with(&named).await else {
            return;
        };
        for to in 2..=MAX_OUTGOING + 1 {
            assert_eq!(
                store.request_friend(1, to, 0).await.unwrap(),
                Ok(Relation::Outgoing)
            );
        }
        assert_eq!(
            store.request_friend(1, MAX_OUTGOING + 2, 0).await.unwrap(),
            Err(FriendError::TooMany)
        );
        store.block(2, 1).await.unwrap().unwrap();
        assert!(store.delete(1).await.unwrap());
        assert!(store.friends_of(3).await.unwrap().people.is_empty());
        assert!(store.friends_of(2).await.unwrap().blocked.is_empty());
    }

    #[tokio::test]
    async fn handles_find_their_accounts_without_case() {
        let Some(store) = store_with(&[(7, "Rook")]).await else {
            return;
        };
        assert_eq!(store.find_handle("rook#0008").await.unwrap(), Some(7));
        for missing in [
            "Rook#0009",
            "Rook",
            "Rook#8",
            "Rook#00008",
            "Ro ok#0008",
            "#0008",
        ] {
            assert_eq!(store.find_handle(missing).await.unwrap(), None, "{missing}");
        }
    }
}
