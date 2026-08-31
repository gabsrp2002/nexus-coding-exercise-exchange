use crate::feed::{AccountId, OrderId, Side};
use serde::{Deserialize, Serialize};

/// Represents an active limit order in the order book.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookOrder {
    pub id: OrderId,
    pub timestamp: u64,
    pub account: AccountId,
    pub symbol: String,
    pub side: Side,
    pub price: f64,
    pub quantity: f64,
    pub initial_quantity: f64,
}

impl BookOrder {
    pub fn new(
        id: OrderId,
        timestamp: u64,
        account: AccountId,
        symbol: String,
        side: Side,
        price: f64,
        quantity: f64,
    ) -> Self {
        Self {
            id,
            timestamp,
            account,
            symbol,
            side,
            price,
            quantity,
            initial_quantity: quantity,
        }
    }
}

/// Represents an executed trade when two crossing orders match.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trade {
    pub id: u64,
    pub timestamp: u64,
    pub symbol: String,
    pub buy_order_id: OrderId,
    pub sell_order_id: OrderId,
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub buy_account: AccountId,
    pub sell_account: AccountId,
    pub price: f64,
    pub quantity: f64,
    pub value: f64,
}

/// Represents an aggregated price level in the order book.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PriceLevelView {
    pub price: f64,
    pub quantity: f64,
    pub order_count: usize,
}

/// Snapshot view of the current state of an order book for a symbol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderBookView {
    pub symbol: String,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
    pub bids: Vec<PriceLevelView>,
    pub asks: Vec<PriceLevelView>,
    pub total_bids_quantity: f64,
    pub total_asks_quantity: f64,
    pub open_orders: Vec<BookOrder>,
}

/// Summary statistics for the matching engine.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct EngineStats {
    pub messages_processed: u64,
    pub orders_received: u64,
    pub cancels_received: u64,
    pub trades_executed: u64,
    pub total_volume_traded: f64,
    pub total_value_traded: f64,
    pub open_orders_count: usize,
}
