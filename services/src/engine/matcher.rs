use crate::engine::book::OrderBook;
use crate::engine::db::Database;
use crate::engine::types::{AccountPortfolio, BookOrder, EngineStats, OrderBookView, Trade};
use crate::feed::{AccountId, OrderMessage, SYMBOLS};
use std::collections::HashMap;
use tracing::info;

/// The central matching engine that maintains order books across symbols,
/// matches orders, executes trades, and manages trade history, statistics,
/// and SQLite persistence for market data and account portfolios.
pub struct MatchingEngine {
    books: HashMap<String, OrderBook>,
    trades: Vec<Trade>,
    stats: EngineStats,
    next_trade_id: u64,
    pub db: Database,
}

impl Default for MatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self::with_db_path(":memory:").expect("Failed to create in-memory database")
    }

    pub fn with_db_path(db_path: &str) -> rusqlite::Result<Self> {
        let mut books = HashMap::new();
        for (symbol, _) in SYMBOLS {
            books.insert(symbol.to_string(), OrderBook::new(symbol.to_string()));
        }
        let mut db = Database::new(db_path)?;

        // Populate initial empty book snapshots in DB
        for book in books.values() {
            let _ = db.save_book_snapshot(&book.to_view());
        }

        Ok(Self {
            books,
            trades: Vec::new(),
            stats: EngineStats::default(),
            next_trade_id: 0,
            db,
        })
    }

    /// Process a feed message (New order or Cancel).
    /// Executes matching logic if it's a new order, or removes the order if cancelled.
    pub fn process_message(&mut self, message: OrderMessage) -> Vec<Trade> {
        self.stats.messages_processed += 1;

        match message {
            OrderMessage::New {
                id,
                timestamp,
                account,
                symbol,
                side,
                price,
                quantity,
            } => {
                self.stats.orders_received += 1;

                let order = BookOrder::new(
                    id,
                    timestamp,
                    account,
                    symbol.clone(),
                    side,
                    price,
                    quantity,
                );

                let book = self
                    .books
                    .entry(symbol.clone())
                    .or_insert_with(|| OrderBook::new(symbol.clone()));

                let executed_trades = book.match_and_insert(order, &mut self.next_trade_id);

                if !executed_trades.is_empty() {
                    let _ = self.db.record_trades(&executed_trades);
                }

                let _ = self.db.save_book_snapshot(&book.to_view());

                for trade in &executed_trades {
                    self.stats.trades_executed += 1;
                    self.stats.total_volume_traded += trade.quantity;
                    self.stats.total_value_traded += trade.value;

                    info!(
                        "⚡ TRADE EXECUTED [{}]: Trade #{} | {} qty @ ${:.2} (${:.2}) | Buyer #{} (Order #{}) <-> Seller #{} (Order #{})",
                        trade.symbol,
                        trade.id,
                        trade.quantity,
                        trade.price,
                        trade.value,
                        trade.buy_account,
                        trade.buy_order_id,
                        trade.sell_account,
                        trade.sell_order_id
                    );

                    self.trades.push(trade.clone());
                }

                self.update_open_orders_count();
                executed_trades
            }
            OrderMessage::Cancel {
                id: _,
                timestamp: _,
                account,
                target_id,
            } => {
                self.stats.cancels_received += 1;
                let mut cancelled = None;

                for book in self.books.values_mut() {
                    if let Some(order) = book.cancel_order(target_id) {
                        let _ = self.db.save_book_snapshot(&book.to_view());
                        cancelled = Some(order);
                        break;
                    }
                }

                if let Some(order) = cancelled {
                    info!(
                        "❌ ORDER CANCELLED: Order #{} ({:?} {} {} @ ${:.2}) by Account #{}",
                        order.id, order.side, order.quantity, order.symbol, order.price, account
                    );
                }

                self.update_open_orders_count();
                Vec::new()
            }
        }
    }

    fn update_open_orders_count(&mut self) {
        self.stats.open_orders_count = self
            .books
            .values()
            .map(|b| b.bids.len() + b.asks.len())
            .sum();
    }

    /// Retrieve a view of the order book for a given symbol.
    pub fn get_book(&self, symbol: &str) -> Option<OrderBookView> {
        self.books.get(symbol).map(|b| b.to_view())
    }

    /// Retrieve views of all order books.
    pub fn get_all_books(&self) -> HashMap<String, OrderBookView> {
        self.books
            .iter()
            .map(|(k, v)| (k.clone(), v.to_view()))
            .collect()
    }

    /// Retrieve trade history with optional symbol and account filter and limit.
    pub fn get_trades(
        &self,
        symbol: Option<&str>,
        account: Option<AccountId>,
        limit: Option<usize>,
    ) -> Vec<Trade> {
        self.db
            .get_trades(symbol, account, limit)
            .unwrap_or_else(|_| {
                let matching_trades = self.trades.iter().rev().filter(|t| {
                    let sym_match = symbol
                        .map(|s| t.symbol.eq_ignore_ascii_case(s))
                        .unwrap_or(true);
                    let acc_match = account
                        .map(|a| t.buy_account == a || t.sell_account == a)
                        .unwrap_or(true);
                    sym_match && acc_match
                });

                if let Some(n) = limit {
                    matching_trades.take(n).cloned().collect()
                } else {
                    matching_trades.cloned().collect()
                }
            })
    }

    /// Retrieve portfolio for a specific account (cash balance, positions, trade count).
    pub fn get_account_portfolio(&self, account_id: AccountId) -> rusqlite::Result<AccountPortfolio> {
        self.db.get_account_portfolio(account_id)
    }

    /// Retrieve all account portfolios.
    pub fn get_all_accounts(&self) -> rusqlite::Result<Vec<AccountPortfolio>> {
        self.db.get_all_accounts()
    }

    /// Retrieve overall engine metrics.
    pub fn get_stats(&self) -> EngineStats {
        self.stats.clone()
    }
}
