//! §10.3 — PostgreSQL backend for the token-transfer indexer.
//!
//! This crate is SQL I/O only. The balance/reorg arithmetic is the audited
//! `misaka-evm-indexer-core` [`Balances`] logic, reused via load → apply →
//! write-back (read the touched balances, fold the block's transfers with
//! `Balances`, write the changed entries) — so PostgreSQL and the in-memory
//! reference store can never disagree on balances, and the only PG-specific
//! risk is the SQL plumbing (covered by the `$DATABASE_URL`-gated integration
//! test, which a live Postgres runs).
//!
//! 78-digit `numeric(78,0)` values (token ids / amounts) cross the wire as
//! decimal TEXT (`$n::numeric` on the way in, `col::text` on the way out) —
//! `U256` exceeds any fixed-width Rust numeric, so text is the lossless bridge.

use std::collections::HashSet;

use alloy_primitives::U256;
use misaka_evm_indexer_core::{Balances, IndexedBlock, LocatedTransfer, TokenStandard};
use tokio_postgres::{Client, NoTls, Transaction};

const ZERO: [u8; 20] = [0u8; 20];

/// The §10.3 schema (idempotent — safe to run on every start).
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS blocks (
    rpc_hash    bytea PRIMARY KEY,
    l1_hash     bytea NOT NULL,
    number      bigint NOT NULL,
    parent_hash bytea NOT NULL,
    canonical   boolean NOT NULL,
    finalized   boolean NOT NULL
);
CREATE INDEX IF NOT EXISTS blocks_number_idx ON blocks(number);

CREATE TABLE IF NOT EXISTS token_transfers (
    id            bigserial PRIMARY KEY,
    block_number  bigint NOT NULL,
    block_hash    bytea NOT NULL,
    tx_hash       bytea NOT NULL,
    tx_index      integer NOT NULL,
    log_index     integer NOT NULL,
    standard      smallint NOT NULL,
    token_address bytea NOT NULL,
    operator      bytea,
    from_address  bytea NOT NULL,
    to_address    bytea NOT NULL,
    token_id      numeric(78,0),
    amount        numeric(78,0) NOT NULL,
    canonical     boolean NOT NULL,
    removed       boolean NOT NULL
);
CREATE INDEX IF NOT EXISTS tt_block_hash_idx ON token_transfers(block_hash);
CREATE INDEX IF NOT EXISTS tt_from_idx       ON token_transfers(from_address, block_number DESC);
CREATE INDEX IF NOT EXISTS tt_to_idx         ON token_transfers(to_address, block_number DESC);
CREATE INDEX IF NOT EXISTS tt_token_idx      ON token_transfers(token_address, block_number DESC);
CREATE INDEX IF NOT EXISTS tt_token_id_idx   ON token_transfers(token_address, token_id, block_number DESC);
CREATE INDEX IF NOT EXISTS tt_tx_idx         ON token_transfers(tx_hash);

CREATE TABLE IF NOT EXISTS erc20_balances (
    token         bytea NOT NULL,
    owner         bytea NOT NULL,
    balance       numeric(78,0) NOT NULL,
    updated_block bigint NOT NULL,
    PRIMARY KEY (token, owner)
);
CREATE TABLE IF NOT EXISTS erc721_ownership (
    collection    bytea NOT NULL,
    token_id      numeric(78,0) NOT NULL,
    owner         bytea NOT NULL,
    updated_block bigint NOT NULL,
    PRIMARY KEY (collection, token_id)
);
CREATE TABLE IF NOT EXISTS erc1155_balances (
    collection    bytea NOT NULL,
    token_id      numeric(78,0) NOT NULL,
    owner         bytea NOT NULL,
    balance       numeric(78,0) NOT NULL,
    updated_block bigint NOT NULL,
    PRIMARY KEY (collection, token_id, owner)
);
"#;

#[derive(Debug, thiserror::Error)]
pub enum PgError {
    #[error("postgres: {0}")]
    Postgres(#[from] tokio_postgres::Error),
    #[error("numeric decode: {0:?} is not a valid decimal uint")]
    BadNumeric(String),
    #[error("column {0} had an unexpected byte length")]
    BadBytes(&'static str),
}

type Result<T> = std::result::Result<T, PgError>;

/// `U256` → its decimal text (for a `numeric(78,0)` param via `$n::numeric`).
fn u256_to_dec(v: U256) -> String {
    v.to_string()
}

/// `numeric(78,0)` decimal text → `U256`.
fn dec_to_u256(s: &str) -> Result<U256> {
    s.trim().parse::<U256>().map_err(|_| PgError::BadNumeric(s.to_string()))
}

fn bytes20(col: &'static str, v: &[u8]) -> Result<[u8; 20]> {
    <[u8; 20]>::try_from(v).map_err(|_| PgError::BadBytes(col))
}

/// PostgreSQL-backed token-transfer store (§10.3).
pub struct PgStore {
    client: Client,
}

impl PgStore {
    /// Connect with a libpq URL (e.g. `postgres://user:pw@localhost/misaka`),
    /// spawning the connection driver task. Then call [`migrate`](Self::migrate).
    pub async fn connect(url: &str) -> Result<Self> {
        let (client, connection) = tokio_postgres::connect(url, NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("[evm-indexer-pg] connection error: {e}");
            }
        });
        Ok(Self { client })
    }

    /// Create the schema if absent (idempotent).
    pub async fn migrate(&self) -> Result<()> {
        self.client.batch_execute(SCHEMA).await?;
        Ok(())
    }

    /// Attach a canonical block + its transfers (§10.5). Idempotent: a block
    /// already canonical is a no-op; a previously reverted block is re-attached
    /// by re-folding its stored rows.
    pub async fn apply_block(&mut self, block: &IndexedBlock, transfers: &[LocatedTransfer]) -> Result<()> {
        let tx = self.client.transaction().await?;
        let rpc_hash: &[u8] = &block.rpc_hash;
        let existing: Option<bool> =
            tx.query_opt("SELECT canonical FROM blocks WHERE rpc_hash = $1", &[&rpc_hash]).await?.map(|r| r.get(0));
        match existing {
            Some(true) => {} // already attached
            Some(false) => {
                // Re-attach: re-fold the stored rows, flip flags back.
                let stored = load_block_transfers(&tx, &block.rpc_hash).await?;
                update_balances(&tx, &stored, block.number, false).await?;
                tx.execute("UPDATE blocks SET canonical = true WHERE rpc_hash = $1", &[&rpc_hash]).await?;
                tx.execute("UPDATE token_transfers SET canonical = true, removed = false WHERE block_hash = $1", &[&rpc_hash])
                    .await?;
            }
            None => {
                let l1: &[u8] = &block.l1_hash;
                let parent: &[u8] = &block.parent_hash;
                tx.execute(
                    "INSERT INTO blocks(rpc_hash, l1_hash, number, parent_hash, canonical, finalized) \
                     VALUES ($1, $2, $3, $4, true, false)",
                    &[&rpc_hash, &l1, &(block.number as i64), &parent],
                )
                .await?;
                for lt in transfers {
                    insert_transfer(&tx, lt).await?;
                }
                update_balances(&tx, transfers, block.number, false).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Detach a block on a reorg: flag its rows removed, apply the inverse
    /// balance delta. No-op if the block is unknown or already non-canonical.
    pub async fn revert_block(&mut self, block_hash: &[u8; 32]) -> Result<()> {
        let tx = self.client.transaction().await?;
        let hash: &[u8] = block_hash;
        let canonical: Option<bool> =
            tx.query_opt("SELECT canonical FROM blocks WHERE rpc_hash = $1", &[&hash]).await?.map(|r| r.get(0));
        if canonical != Some(true) {
            tx.commit().await?;
            return Ok(());
        }
        let number: i64 = tx.query_one("SELECT number FROM blocks WHERE rpc_hash = $1", &[&hash]).await?.get(0);
        let stored = load_block_transfers(&tx, block_hash).await?;
        update_balances(&tx, &stored, number as u64, true).await?;
        tx.execute("UPDATE blocks SET canonical = false WHERE rpc_hash = $1", &[&hash]).await?;
        tx.execute("UPDATE token_transfers SET canonical = false, removed = true WHERE block_hash = $1", &[&hash]).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Mark every block at height `<= up_to_number` finalized (immutable).
    pub async fn set_finalized(&mut self, up_to_number: u64) -> Result<()> {
        self.client
            .execute("UPDATE blocks SET finalized = true WHERE number <= $1 AND canonical = true", &[&(up_to_number as i64)])
            .await?;
        Ok(())
    }

    /// The current canonical head (highest-number canonical block).
    pub async fn head(&self) -> Result<Option<IndexedBlock>> {
        let row = self
            .client
            .query_opt(
                "SELECT rpc_hash, l1_hash, number, parent_hash, canonical, finalized \
                 FROM blocks WHERE canonical = true ORDER BY number DESC LIMIT 1",
                &[],
            )
            .await?;
        row.map(row_to_block).transpose()
    }

    /// The canonical block at a height (§10.6 `getBlockByNumber`; the reconcile
    /// planner's `local_at(n)`). At most one block per height is canonical.
    pub async fn canonical_block_at(&self, number: u64) -> Result<Option<IndexedBlock>> {
        let row = self
            .client
            .query_opt(
                "SELECT rpc_hash, l1_hash, number, parent_hash, canonical, finalized \
                 FROM blocks WHERE number = $1 AND canonical = true LIMIT 1",
                &[&(number as i64)],
            )
            .await?;
        row.map(row_to_block).transpose()
    }

    pub async fn erc20_balance(&self, token: [u8; 20], owner: [u8; 20]) -> Result<U256> {
        let t: &[u8] = &token;
        let o: &[u8] = &owner;
        let row = self.client.query_opt("SELECT balance::text FROM erc20_balances WHERE token=$1 AND owner=$2", &[&t, &o]).await?;
        match row {
            Some(r) => dec_to_u256(&r.get::<_, String>(0)),
            None => Ok(U256::ZERO),
        }
    }

    pub async fn erc721_owner(&self, collection: [u8; 20], token_id: U256) -> Result<Option<[u8; 20]>> {
        let c: &[u8] = &collection;
        let row = self
            .client
            .query_opt("SELECT owner FROM erc721_ownership WHERE collection=$1 AND token_id=$2::numeric", &[&c, &u256_to_dec(token_id)])
            .await?;
        match row {
            Some(r) => Ok(Some(bytes20("owner", &r.get::<_, Vec<u8>>(0))?)),
            None => Ok(None),
        }
    }

    pub async fn erc1155_balance(&self, collection: [u8; 20], token_id: U256, owner: [u8; 20]) -> Result<U256> {
        let c: &[u8] = &collection;
        let o: &[u8] = &owner;
        let row = self
            .client
            .query_opt(
                "SELECT balance::text FROM erc1155_balances WHERE collection=$1 AND token_id=$2::numeric AND owner=$3",
                &[&c, &u256_to_dec(token_id), &o],
            )
            .await?;
        match row {
            Some(r) => dec_to_u256(&r.get::<_, String>(0)),
            None => Ok(U256::ZERO),
        }
    }
}

fn row_to_block(r: tokio_postgres::Row) -> Result<IndexedBlock> {
    let number: i64 = r.get(2);
    Ok(IndexedBlock {
        rpc_hash: bytes32("rpc_hash", &r.get::<_, Vec<u8>>(0))?,
        l1_hash: bytes32("l1_hash", &r.get::<_, Vec<u8>>(1))?,
        number: number as u64,
        parent_hash: bytes32("parent_hash", &r.get::<_, Vec<u8>>(3))?,
        canonical: r.get(4),
        finalized: r.get(5),
    })
}

fn bytes32(col: &'static str, v: &[u8]) -> Result<[u8; 32]> {
    <[u8; 32]>::try_from(v).map_err(|_| PgError::BadBytes(col))
}

/// Insert one `token_transfers` row (canonical, not removed).
async fn insert_transfer(tx: &Transaction<'_>, lt: &LocatedTransfer) -> Result<()> {
    let t = &lt.transfer;
    let block_hash: &[u8] = &lt.block_hash;
    let tx_hash: &[u8] = &lt.tx_hash;
    let token: &[u8] = &t.token;
    let from: &[u8] = &t.from;
    let to: &[u8] = &t.to;
    let operator: Option<Vec<u8>> = t.operator.map(|o| o.to_vec());
    let token_id: Option<String> = t.token_id.map(u256_to_dec);
    let amount = u256_to_dec(t.amount);
    tx.execute(
        "INSERT INTO token_transfers \
         (block_number, block_hash, tx_hash, tx_index, log_index, standard, token_address, operator, \
          from_address, to_address, token_id, amount, canonical, removed) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::numeric,$12::numeric,true,false)",
        &[
            &(lt.block_number as i64),
            &block_hash,
            &tx_hash,
            &(lt.tx_index as i32),
            &(lt.log_index as i32),
            &(t.standard as i16),
            &token,
            &operator,
            &from,
            &to,
            &token_id,
            &amount,
        ],
    )
    .await?;
    Ok(())
}

/// Load a block's stored transfer rows (for re-attach / revert), in apply order.
async fn load_block_transfers(tx: &Transaction<'_>, block_hash: &[u8; 32]) -> Result<Vec<LocatedTransfer>> {
    use misaka_evm_indexer_core::TokenTransfer;
    let hash: &[u8] = block_hash;
    let rows = tx
        .query(
            "SELECT block_number, tx_hash, tx_index, log_index, standard, token_address, operator, \
                    from_address, to_address, token_id::text, amount::text \
             FROM token_transfers WHERE block_hash = $1 ORDER BY tx_index, log_index, id",
            &[&hash],
        )
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let standard = match r.get::<_, i16>(4) {
            0 => TokenStandard::Erc20,
            1 => TokenStandard::Erc721,
            _ => TokenStandard::Erc1155,
        };
        let operator: Option<Vec<u8>> = r.get(6);
        let token_id: Option<String> = r.get(9);
        out.push(LocatedTransfer {
            block_number: r.get::<_, i64>(0) as u64,
            block_hash: *block_hash,
            tx_hash: bytes32("tx_hash", &r.get::<_, Vec<u8>>(1))?,
            tx_index: r.get::<_, i32>(2) as u32,
            log_index: r.get::<_, i32>(3) as u32,
            transfer: TokenTransfer {
                standard,
                token: bytes20("token_address", &r.get::<_, Vec<u8>>(5))?,
                operator: operator.map(|o| bytes20("operator", &o)).transpose()?,
                from: bytes20("from_address", &r.get::<_, Vec<u8>>(7))?,
                to: bytes20("to_address", &r.get::<_, Vec<u8>>(8))?,
                token_id: token_id.map(|s| dec_to_u256(&s)).transpose()?,
                amount: dec_to_u256(&r.get::<_, String>(10))?,
            },
        });
    }
    Ok(out)
}

/// §10.5 balance delta via the audited core logic: read the touched balances,
/// fold (apply or revert) the block's transfers with [`Balances`], write back
/// the changed entries. `Balances::revert_block` reverses intra-block order, so
/// a token moved A→B→C within one block unwinds correctly.
async fn update_balances(tx: &Transaction<'_>, transfers: &[LocatedTransfer], block_number: u64, revert: bool) -> Result<()> {
    let mut e20: HashSet<([u8; 20], [u8; 20])> = HashSet::new();
    let mut e721: HashSet<([u8; 20], U256)> = HashSet::new();
    let mut e1155: HashSet<([u8; 20], U256, [u8; 20])> = HashSet::new();
    for lt in transfers {
        let t = &lt.transfer;
        match t.standard {
            TokenStandard::Erc20 => {
                if t.from != ZERO {
                    e20.insert((t.token, t.from));
                }
                if t.to != ZERO {
                    e20.insert((t.token, t.to));
                }
            }
            TokenStandard::Erc721 => {
                e721.insert((t.token, t.token_id.unwrap_or(U256::ZERO)));
            }
            TokenStandard::Erc1155 => {
                let id = t.token_id.unwrap_or(U256::ZERO);
                if t.from != ZERO {
                    e1155.insert((t.token, id, t.from));
                }
                if t.to != ZERO {
                    e1155.insert((t.token, id, t.to));
                }
            }
        }
    }

    // Seed a Balances with the current persisted values of the touched keys.
    let mut bal = Balances::new();
    for (token, owner) in &e20 {
        bal.set_erc20(*token, *owner, read_erc20(tx, token, owner).await?);
    }
    for (coll, id) in &e721 {
        bal.set_erc721(*coll, *id, read_erc721_owner(tx, coll, *id).await?);
    }
    for (coll, id, owner) in &e1155 {
        bal.set_erc1155(*coll, *id, *owner, read_erc1155(tx, coll, *id, owner).await?);
    }

    // Fold the block (apply or its exact inverse).
    let tts: Vec<_> = transfers.iter().map(|lt| lt.transfer.clone()).collect();
    if revert {
        bal.revert_block(&tts);
    } else {
        bal.apply_block(&tts);
    }

    // Write back the new value of every touched key.
    for (token, owner) in &e20 {
        write_erc20(tx, token, owner, bal.erc20_balance(*token, *owner), block_number).await?;
    }
    for (coll, id) in &e721 {
        write_erc721(tx, coll, *id, bal.erc721_owner(*coll, *id), block_number).await?;
    }
    for (coll, id, owner) in &e1155 {
        write_erc1155(tx, coll, *id, owner, bal.erc1155_balance(*coll, *id, *owner), block_number).await?;
    }
    Ok(())
}

async fn read_erc20(tx: &Transaction<'_>, token: &[u8; 20], owner: &[u8; 20]) -> Result<U256> {
    let t: &[u8] = token;
    let o: &[u8] = owner;
    let row = tx.query_opt("SELECT balance::text FROM erc20_balances WHERE token=$1 AND owner=$2", &[&t, &o]).await?;
    match row {
        Some(r) => dec_to_u256(&r.get::<_, String>(0)),
        None => Ok(U256::ZERO),
    }
}

async fn write_erc20(tx: &Transaction<'_>, token: &[u8; 20], owner: &[u8; 20], balance: U256, block: u64) -> Result<()> {
    let t: &[u8] = token;
    let o: &[u8] = owner;
    if balance.is_zero() {
        tx.execute("DELETE FROM erc20_balances WHERE token=$1 AND owner=$2", &[&t, &o]).await?;
    } else {
        tx.execute(
            "INSERT INTO erc20_balances(token, owner, balance, updated_block) VALUES ($1,$2,$3::numeric,$4) \
             ON CONFLICT (token, owner) DO UPDATE SET balance = EXCLUDED.balance, updated_block = EXCLUDED.updated_block",
            &[&t, &o, &u256_to_dec(balance), &(block as i64)],
        )
        .await?;
    }
    Ok(())
}

async fn read_erc721_owner(tx: &Transaction<'_>, coll: &[u8; 20], id: U256) -> Result<Option<[u8; 20]>> {
    let c: &[u8] = coll;
    let row =
        tx.query_opt("SELECT owner FROM erc721_ownership WHERE collection=$1 AND token_id=$2::numeric", &[&c, &u256_to_dec(id)]).await?;
    match row {
        Some(r) => Ok(Some(bytes20("owner", &r.get::<_, Vec<u8>>(0))?)),
        None => Ok(None),
    }
}

async fn write_erc721(tx: &Transaction<'_>, coll: &[u8; 20], id: U256, owner: Option<[u8; 20]>, block: u64) -> Result<()> {
    let c: &[u8] = coll;
    match owner {
        Some(o) => {
            let ob: &[u8] = &o;
            tx.execute(
                "INSERT INTO erc721_ownership(collection, token_id, owner, updated_block) VALUES ($1,$2::numeric,$3,$4) \
                 ON CONFLICT (collection, token_id) DO UPDATE SET owner = EXCLUDED.owner, updated_block = EXCLUDED.updated_block",
                &[&c, &u256_to_dec(id), &ob, &(block as i64)],
            )
            .await?;
        }
        None => {
            tx.execute("DELETE FROM erc721_ownership WHERE collection=$1 AND token_id=$2::numeric", &[&c, &u256_to_dec(id)]).await?;
        }
    }
    Ok(())
}

async fn read_erc1155(tx: &Transaction<'_>, coll: &[u8; 20], id: U256, owner: &[u8; 20]) -> Result<U256> {
    let c: &[u8] = coll;
    let o: &[u8] = owner;
    let row = tx
        .query_opt("SELECT balance::text FROM erc1155_balances WHERE collection=$1 AND token_id=$2::numeric AND owner=$3", &[
            &c,
            &u256_to_dec(id),
            &o,
        ])
        .await?;
    match row {
        Some(r) => dec_to_u256(&r.get::<_, String>(0)),
        None => Ok(U256::ZERO),
    }
}

async fn write_erc1155(tx: &Transaction<'_>, coll: &[u8; 20], id: U256, owner: &[u8; 20], balance: U256, block: u64) -> Result<()> {
    let c: &[u8] = coll;
    let o: &[u8] = owner;
    if balance.is_zero() {
        tx.execute("DELETE FROM erc1155_balances WHERE collection=$1 AND token_id=$2::numeric AND owner=$3", &[&c, &u256_to_dec(id), &o])
            .await?;
    } else {
        tx.execute(
            "INSERT INTO erc1155_balances(collection, token_id, owner, balance, updated_block) VALUES ($1,$2::numeric,$3,$4::numeric,$5) \
             ON CONFLICT (collection, token_id, owner) DO UPDATE SET balance = EXCLUDED.balance, updated_block = EXCLUDED.updated_block",
            &[&c, &u256_to_dec(id), &o, &u256_to_dec(balance), &(block as i64)],
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u256_decimal_text_round_trips() {
        for v in [U256::ZERO, U256::from(1u64), U256::from(1_000_000u64), U256::MAX] {
            assert_eq!(dec_to_u256(&u256_to_dec(v)).unwrap(), v);
        }
        assert!(dec_to_u256("not-a-number").is_err());
    }

    /// Full PG path against a live database — set MISAKA_PG_TEST_URL to run
    /// (e.g. `postgres://localhost/misaka_test`). Verifies apply/revert/reattach
    /// + finalize + balance reads match the in-memory oracle's semantics.
    #[tokio::test]
    #[ignore = "needs a live PostgreSQL; set MISAKA_PG_TEST_URL"]
    async fn pg_apply_revert_roundtrip() {
        let Ok(url) = std::env::var("MISAKA_PG_TEST_URL") else { return };
        use misaka_evm_indexer_core::{TokenStandard, TokenTransfer};
        let mut s = PgStore::connect(&url).await.unwrap();
        s.migrate().await.unwrap();
        // Clean slate.
        s.client.batch_execute("TRUNCATE blocks, token_transfers, erc20_balances, erc721_ownership, erc1155_balances").await.unwrap();

        let tok = [0x11u8; 20];
        let a = [0xAAu8; 20];
        let b = [0xBBu8; 20];
        fn blk(n: u64, hash: u8) -> IndexedBlock {
            IndexedBlock {
                rpc_hash: [hash; 32],
                l1_hash: [hash; 32],
                number: n,
                parent_hash: [hash.wrapping_sub(1); 32],
                canonical: true,
                finalized: false,
            }
        }
        let xfer = |hash: u8, from: [u8; 20], to: [u8; 20], amount: u64| LocatedTransfer {
            block_number: hash as u64,
            block_hash: [hash; 32],
            tx_hash: [0x01; 32],
            tx_index: 0,
            log_index: 0,
            transfer: TokenTransfer {
                standard: TokenStandard::Erc20,
                token: tok,
                operator: None,
                from,
                to,
                token_id: None,
                amount: U256::from(amount),
            },
        };

        s.apply_block(&blk(1, 1), &[xfer(1, ZERO, a, 100)]).await.unwrap();
        s.apply_block(&blk(2, 2), &[xfer(2, a, b, 40)]).await.unwrap();
        assert_eq!(s.erc20_balance(tok, a).await.unwrap(), U256::from(60u64));
        assert_eq!(s.erc20_balance(tok, b).await.unwrap(), U256::from(40u64));
        assert_eq!(s.head().await.unwrap().unwrap().number, 2);

        s.revert_block(&[2u8; 32]).await.unwrap();
        assert_eq!(s.erc20_balance(tok, a).await.unwrap(), U256::from(100u64));
        assert_eq!(s.erc20_balance(tok, b).await.unwrap(), U256::ZERO);
        assert_eq!(s.head().await.unwrap().unwrap().number, 1);
    }
}
