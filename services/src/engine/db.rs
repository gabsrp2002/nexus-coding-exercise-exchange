use crate::engine::types::{AccountPortfolio, OrderBookView, PositionView, Trade};
use crate::feed::AccountId;
use rusqlite::{Connection, OptionalExtension, Result, params};
use serde_json;

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// SQLite-backed database manager for market data, trade records,
/// book snapshots, and account positions / cash balances.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens or creates a SQLite database at the specified path (or ":memory:").
    pub fn new(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };

        let mut db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Initializes tables and indexes if they do not exist.
    fn init_schema(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS trades (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                symbol TEXT NOT NULL,
                buy_order_id INTEGER NOT NULL,
                sell_order_id INTEGER NOT NULL,
                maker_order_id INTEGER NOT NULL,
                taker_order_id INTEGER NOT NULL,
                buy_account INTEGER NOT NULL,
                sell_account INTEGER NOT NULL,
                price REAL NOT NULL,
                quantity REAL NOT NULL,
                value REAL NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_trades_symbol ON trades (symbol);
            CREATE INDEX IF NOT EXISTS idx_trades_buy_account ON trades (buy_account);
            CREATE INDEX IF NOT EXISTS idx_trades_sell_account ON trades (sell_account);
            CREATE INDEX IF NOT EXISTS idx_trades_timestamp ON trades (timestamp);

            CREATE TABLE IF NOT EXISTS account_balances (
                account_id INTEGER PRIMARY KEY,
                cash REAL NOT NULL DEFAULT 0.0
            );

            CREATE TABLE IF NOT EXISTS account_positions (
                account_id INTEGER NOT NULL,
                symbol TEXT NOT NULL,
                position REAL NOT NULL DEFAULT 0.0,
                PRIMARY KEY (account_id, symbol)
            );

            CREATE TABLE IF NOT EXISTS book_snapshots (
                symbol TEXT PRIMARY KEY,
                best_bid REAL,
                best_ask REAL,
                spread REAL,
                snapshot_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    /// Persists trades to the database and updates account cash and token positions atomically.
    pub fn record_trades(&mut self, trades: &[Trade]) -> Result<()> {
        if trades.is_empty() {
            return Ok(());
        }

        let tx = self.conn.transaction()?;

        {
            let mut insert_trade = tx.prepare_cached(
                "INSERT INTO trades (
                    id, timestamp, symbol, buy_order_id, sell_order_id,
                    maker_order_id, taker_order_id, buy_account, sell_account,
                    price, quantity, value
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ON CONFLICT(id) DO NOTHING",
            )?;

            let mut update_cash = tx.prepare_cached(
                "INSERT INTO account_balances (account_id, cash)
                 VALUES (?1, ?2)
                 ON CONFLICT(account_id) DO UPDATE SET cash = round(cash + ?2, 2)",
            )?;

            let mut update_pos = tx.prepare_cached(
                "INSERT INTO account_positions (account_id, symbol, position)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(account_id, symbol) DO UPDATE SET position = round(position + ?3, 1)",
            )?;

            for trade in trades {
                insert_trade.execute(params![
                    trade.id,
                    trade.timestamp,
                    trade.symbol,
                    trade.buy_order_id,
                    trade.sell_order_id,
                    trade.maker_order_id,
                    trade.taker_order_id,
                    trade.buy_account,
                    trade.sell_account,
                    trade.price,
                    trade.quantity,
                    trade.value,
                ])?;

                // Buyer pays quote cash (cash decreases), gains base asset position
                update_cash.execute(params![trade.buy_account, -trade.value])?;
                update_pos.execute(params![trade.buy_account, trade.symbol, trade.quantity])?;

                // Seller receives quote cash (cash increases), gives base asset position
                update_cash.execute(params![trade.sell_account, trade.value])?;
                update_pos.execute(params![trade.sell_account, trade.symbol, -trade.quantity])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Saves or updates the latest order book snapshot for a symbol.
    pub fn save_book_snapshot(&mut self, book: &OrderBookView) -> Result<()> {
        let snapshot_json = serde_json::to_string(book).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
        })?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        self.conn.execute(
            "INSERT INTO book_snapshots (symbol, best_bid, best_ask, spread, snapshot_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(symbol) DO UPDATE SET
                best_bid = excluded.best_bid,
                best_ask = excluded.best_ask,
                spread = excluded.spread,
                snapshot_json = excluded.snapshot_json,
                updated_at = excluded.updated_at",
            params![
                book.symbol,
                book.best_bid,
                book.best_ask,
                book.spread,
                snapshot_json,
                now,
            ],
        )?;

        Ok(())
    }

    /// Retrieves trade history with optional filters on symbol and account, limited to `limit` trades.
    pub fn get_trades(
        &self,
        symbol: Option<&str>,
        account: Option<AccountId>,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        let mut sql = String::from(
            "SELECT id, timestamp, symbol, buy_order_id, sell_order_id,
                    maker_order_id, taker_order_id, buy_account, sell_account,
                    price, quantity, value
             FROM trades WHERE 1=1",
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(sym) = symbol {
            sql.push_str(" AND symbol = ?");
            params_vec.push(Box::new(sym.to_string()));
        }

        if let Some(acc) = account {
            sql.push_str(" AND (buy_account = ? OR sell_account = ?)");
            params_vec.push(Box::new(acc));
            params_vec.push(Box::new(acc));
        }

        sql.push_str(" ORDER BY id DESC");

        if let Some(n) = limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(Trade {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                symbol: row.get(2)?,
                buy_order_id: row.get(3)?,
                sell_order_id: row.get(4)?,
                maker_order_id: row.get(5)?,
                taker_order_id: row.get(6)?,
                buy_account: row.get(7)?,
                sell_account: row.get(8)?,
                price: row.get(9)?,
                quantity: row.get(10)?,
                value: row.get(11)?,
            })
        })?;

        let mut trades = Vec::new();
        for r in rows {
            trades.push(r?);
        }
        Ok(trades)
    }

    /// Retrieves an account's portfolio including notional cash, asset positions, and total fill counts.
    pub fn get_account_portfolio(&self, account_id: AccountId) -> Result<AccountPortfolio> {
        let cash: f64 = self
            .conn
            .query_row(
                "SELECT cash FROM account_balances WHERE account_id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0.0);

        let mut stmt = self.conn.prepare(
            "SELECT symbol, position FROM account_positions WHERE account_id = ?1 ORDER BY symbol ASC",
        )?;

        let pos_rows = stmt.query_map(params![account_id], |row| {
            Ok(PositionView {
                symbol: row.get(0)?,
                position: round1(row.get(1)?),
            })
        })?;

        let mut positions = Vec::new();
        for pos in pos_rows {
            positions.push(pos?);
        }

        let trades_count: usize = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM trades WHERE buy_account = ?1 OR sell_account = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(AccountPortfolio {
            account_id,
            cash: round2(cash),
            positions,
            trades_count,
        })
    }

    /// Retrieves portfolios for all active accounts with trades or positions.
    pub fn get_all_accounts(&self) -> Result<Vec<AccountPortfolio>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT account_id FROM (
                SELECT account_id FROM account_balances
                UNION
                SELECT account_id FROM account_positions
                UNION
                SELECT buy_account AS account_id FROM trades
                UNION
                SELECT sell_account AS account_id FROM trades
            ) ORDER BY account_id ASC",
        )?;

        let acc_rows = stmt.query_map([], |row| row.get::<_, AccountId>(0))?;

        let mut accounts = Vec::new();
        for acc in acc_rows {
            let account_id = acc?;
            accounts.push(self.get_account_portfolio(account_id)?);
        }
        Ok(accounts)
    }

    /// Retrieves the saved book snapshot for a symbol.
    pub fn get_book_snapshot(&self, symbol: &str) -> Result<Option<OrderBookView>> {
        let snapshot_json: Option<String> = self
            .conn
            .query_row(
                "SELECT snapshot_json FROM book_snapshots WHERE symbol = ?1",
                params![symbol],
                |row| row.get(0),
            )
            .optional()?;

        match snapshot_json {
            Some(json_str) => {
                let view: OrderBookView = serde_json::from_str(&json_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                Ok(Some(view))
            }
            None => Ok(None),
        }
    }
}
