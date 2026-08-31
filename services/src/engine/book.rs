use crate::engine::types::{BookOrder, OrderBookView, PriceLevelView, Trade};
use crate::feed::{OrderId, Side};
use std::cmp::Ordering;

const EPSILON: f64 = 1e-7;

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Compare buy orders for price-time priority:
/// Higher price has higher priority (comes earlier).
/// If prices are equal, lower ID / earlier arrival comes earlier.
fn compare_bids(a: &BookOrder, b: &BookOrder) -> Ordering {
    match b.price.total_cmp(&a.price) {
        Ordering::Equal => a.id.cmp(&b.id),
        other => other,
    }
}

/// Compare sell orders for price-time priority:
/// Lower price has higher priority (comes earlier).
/// If prices are equal, lower ID / earlier arrival comes earlier.
fn compare_asks(a: &BookOrder, b: &BookOrder) -> Ordering {
    match a.price.total_cmp(&b.price) {
        Ordering::Equal => a.id.cmp(&b.id),
        other => other,
    }
}

/// Manages the bid and ask books for a single trading symbol.
#[derive(Debug, Clone)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<BookOrder>,
    pub asks: Vec<BookOrder>,
}

impl OrderBook {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }

    /// Processes an incoming order:
    /// - Matches against crossing resting orders in the book using price-time priority.
    /// - Generates trades at maker prices.
    /// - Any remaining unfilled quantity is placed in the book as a resting order.
    pub fn match_and_insert(
        &mut self,
        mut order: BookOrder,
        next_trade_id: &mut u64,
    ) -> Vec<Trade> {
        let mut executed_trades = Vec::new();

        match order.side {
            Side::Buy => {
                // Incoming Buy matches resting Asks where ask.price <= order.price
                while order.quantity > EPSILON && !self.asks.is_empty() {
                    let best_ask = &mut self.asks[0];
                    if best_ask.price > order.price + EPSILON {
                        // Best ask price is higher than buy limit price; no match
                        break;
                    }

                    let match_price = best_ask.price; // Maker price
                    let match_qty = round1(f64::min(order.quantity, best_ask.quantity));

                    if match_qty <= EPSILON {
                        break;
                    }

                    *next_trade_id += 1;
                    let trade = Trade {
                        id: *next_trade_id,
                        timestamp: order.timestamp,
                        symbol: self.symbol.clone(),
                        buy_order_id: order.id,
                        sell_order_id: best_ask.id,
                        maker_order_id: best_ask.id,
                        taker_order_id: order.id,
                        buy_account: order.account,
                        sell_account: best_ask.account,
                        price: round2(match_price),
                        quantity: match_qty,
                        value: round2(match_price * match_qty),
                    };

                    order.quantity = round1(order.quantity - match_qty);
                    best_ask.quantity = round1(best_ask.quantity - match_qty);

                    executed_trades.push(trade);

                    if best_ask.quantity <= EPSILON {
                        self.asks.remove(0);
                    }
                }

                // If unfilled balance remains, insert into bids book
                if order.quantity > EPSILON {
                    self.insert_bid(order);
                }
            }
            Side::Sell => {
                // Incoming Sell matches resting Bids where bid.price >= order.price
                while order.quantity > EPSILON && !self.bids.is_empty() {
                    let best_bid = &mut self.bids[0];
                    if best_bid.price < order.price - EPSILON {
                        // Best bid price is lower than sell limit price; no match
                        break;
                    }

                    let match_price = best_bid.price; // Maker price
                    let match_qty = round1(f64::min(order.quantity, best_bid.quantity));

                    if match_qty <= EPSILON {
                        break;
                    }

                    *next_trade_id += 1;
                    let trade = Trade {
                        id: *next_trade_id,
                        timestamp: order.timestamp,
                        symbol: self.symbol.clone(),
                        buy_order_id: best_bid.id,
                        sell_order_id: order.id,
                        maker_order_id: best_bid.id,
                        taker_order_id: order.id,
                        buy_account: best_bid.account,
                        sell_account: order.account,
                        price: round2(match_price),
                        quantity: match_qty,
                        value: round2(match_price * match_qty),
                    };

                    order.quantity = round1(order.quantity - match_qty);
                    best_bid.quantity = round1(best_bid.quantity - match_qty);

                    executed_trades.push(trade);

                    if best_bid.quantity <= EPSILON {
                        self.bids.remove(0);
                    }
                }

                // If unfilled balance remains, insert into asks book
                if order.quantity > EPSILON {
                    self.insert_ask(order);
                }
            }
        }

        executed_trades
    }

    /// Inserts a buy order into the sorted bids list (price desc, time asc).
    fn insert_bid(&mut self, order: BookOrder) {
        let pos = self
            .bids
            .binary_search_by(|b| compare_bids(b, &order))
            .unwrap_or_else(|pos| pos);
        self.bids.insert(pos, order);
    }

    /// Inserts a sell order into the sorted asks list (price asc, time asc).
    fn insert_ask(&mut self, order: BookOrder) {
        let pos = self
            .asks
            .binary_search_by(|a| compare_asks(a, &order))
            .unwrap_or_else(|pos| pos);
        self.asks.insert(pos, order);
    }

    /// Cancels an order by ID if present in the book.
    pub fn cancel_order(&mut self, order_id: OrderId) -> Option<BookOrder> {
        if let Some(idx) = self.bids.iter().position(|o| o.id == order_id) {
            return Some(self.bids.remove(idx));
        }
        if let Some(idx) = self.asks.iter().position(|o| o.id == order_id) {
            return Some(self.asks.remove(idx));
        }
        None
    }

    /// Returns the best bid price, if any.
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|o| o.price)
    }

    /// Returns the best ask price, if any.
    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|o| o.price)
    }

    /// Returns the current spread between best ask and best bid.
    pub fn spread(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(round2(ask - bid)),
            _ => None,
        }
    }

    /// Returns aggregated price levels for bids (grouped by price).
    pub fn aggregate_bids(&self) -> Vec<PriceLevelView> {
        let mut levels: Vec<PriceLevelView> = Vec::new();
        for order in &self.bids {
            if let Some(last) = levels
                .last_mut()
                .filter(|l| (l.price - order.price).abs() < EPSILON)
            {
                last.quantity = round1(last.quantity + order.quantity);
                last.order_count += 1;
                continue;
            }
            levels.push(PriceLevelView {
                price: order.price,
                quantity: order.quantity,
                order_count: 1,
            });
        }
        levels
    }

    /// Returns aggregated price levels for asks (grouped by price).
    pub fn aggregate_asks(&self) -> Vec<PriceLevelView> {
        let mut levels: Vec<PriceLevelView> = Vec::new();
        for order in &self.asks {
            if let Some(last) = levels
                .last_mut()
                .filter(|l| (l.price - order.price).abs() < EPSILON)
            {
                last.quantity = round1(last.quantity + order.quantity);
                last.order_count += 1;
                continue;
            }
            levels.push(PriceLevelView {
                price: order.price,
                quantity: order.quantity,
                order_count: 1,
            });
        }
        levels
    }

    /// Produces a snapshot view of the order book.
    pub fn to_view(&self) -> OrderBookView {
        let bids_view = self.aggregate_bids();
        let asks_view = self.aggregate_asks();
        let total_bids_quantity = round1(bids_view.iter().map(|l| l.quantity).sum());
        let total_asks_quantity = round1(asks_view.iter().map(|l| l.quantity).sum());

        let mut all_orders = Vec::with_capacity(self.bids.len() + self.asks.len());
        all_orders.extend(self.bids.clone());
        all_orders.extend(self.asks.clone());

        OrderBookView {
            symbol: self.symbol.clone(),
            best_bid: self.best_bid(),
            best_ask: self.best_ask(),
            spread: self.spread(),
            bids: bids_view,
            asks: asks_view,
            total_bids_quantity,
            total_asks_quantity,
            open_orders: all_orders,
        }
    }
}
