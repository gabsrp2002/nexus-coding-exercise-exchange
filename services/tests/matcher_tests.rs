use services::engine::MatchingEngine;
use services::feed::{OrderMessage, Side};

#[test]
fn test_exact_match() {
    let mut engine = MatchingEngine::new();

    // 1. Account 1 places Sell 5.0 ETH-USDC @ 100.0 (maker)
    let msg1 = OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 100.0,
        quantity: 5.0,
    };
    let trades1 = engine.process_message(msg1);
    assert_eq!(trades1.len(), 0);

    let book = engine.get_book("ETH-USDC").unwrap();
    assert_eq!(book.asks.len(), 1);
    assert_eq!(book.bids.len(), 0);
    assert_eq!(book.best_ask, Some(100.0));

    // 2. Account 2 places Buy 5.0 ETH-USDC @ 100.0 (taker)
    let msg2 = OrderMessage::New {
        id: 2,
        timestamp: 2000,
        account: 2,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 100.0,
        quantity: 5.0,
    };
    let trades2 = engine.process_message(msg2);
    assert_eq!(trades2.len(), 1);

    let trade = &trades2[0];
    assert_eq!(trade.id, 1);
    assert_eq!(trade.symbol, "ETH-USDC");
    assert_eq!(trade.buy_order_id, 2);
    assert_eq!(trade.sell_order_id, 1);
    assert_eq!(trade.maker_order_id, 1);
    assert_eq!(trade.taker_order_id, 2);
    assert_eq!(trade.buy_account, 2);
    assert_eq!(trade.sell_account, 1);
    assert_eq!(trade.price, 100.0);
    assert_eq!(trade.quantity, 5.0);
    assert_eq!(trade.value, 500.0);

    // Book should now be completely empty
    let book = engine.get_book("ETH-USDC").unwrap();
    assert_eq!(book.asks.len(), 0);
    assert_eq!(book.bids.len(), 0);
    assert_eq!(book.open_orders.len(), 0);
}

#[test]
fn test_partial_fill_taker_larger() {
    let mut engine = MatchingEngine::new();

    // Maker sells 3.0 @ 100.0
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 100.0,
        quantity: 3.0,
    });

    // Taker buys 5.0 @ 100.0
    let trades = engine.process_message(OrderMessage::New {
        id: 2,
        timestamp: 2000,
        account: 2,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 100.0,
        quantity: 5.0,
    });

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].quantity, 3.0);
    assert_eq!(trades[0].price, 100.0);

    // Asks are cleared, remaining 2.0 buy rests in book
    let book = engine.get_book("ETH-USDC").unwrap();
    assert_eq!(book.asks.len(), 0);
    assert_eq!(book.bids.len(), 1);
    assert_eq!(book.bids[0].quantity, 2.0);
    assert_eq!(book.bids[0].price, 100.0);
    assert_eq!(book.best_bid, Some(100.0));
}

#[test]
fn test_partial_fill_maker_larger() {
    let mut engine = MatchingEngine::new();

    // Maker buys 10.0 @ 50.0
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 1,
        symbol: "NEX-USDC".to_string(),
        side: Side::Buy,
        price: 50.0,
        quantity: 10.0,
    });

    // Taker sells 4.0 @ 50.0
    let trades = engine.process_message(OrderMessage::New {
        id: 2,
        timestamp: 2000,
        account: 2,
        symbol: "NEX-USDC".to_string(),
        side: Side::Sell,
        price: 50.0,
        quantity: 4.0,
    });

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].quantity, 4.0);
    assert_eq!(trades[0].price, 50.0);

    // Resting bid remaining quantity is 6.0
    let book = engine.get_book("NEX-USDC").unwrap();
    assert_eq!(book.bids.len(), 1);
    assert_eq!(book.bids[0].quantity, 6.0);
    assert_eq!(book.asks.len(), 0);
}

#[test]
fn test_maker_pricing() {
    let mut engine = MatchingEngine::new();

    // Resting Buy at 105.0
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 105.0,
        quantity: 2.0,
    });

    // Incoming Sell at 100.0 matches at maker's price (105.0)
    let trades = engine.process_message(OrderMessage::New {
        id: 2,
        timestamp: 2000,
        account: 2,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 100.0,
        quantity: 2.0,
    });

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].price, 105.0);
    assert_eq!(trades[0].maker_order_id, 1);
    assert_eq!(trades[0].taker_order_id, 2);
}

#[test]
fn test_price_time_priority() {
    let mut engine = MatchingEngine::new();

    // Ask 1: 102.0 (qty 2.0)
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 100,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 102.0,
        quantity: 2.0,
    });

    // Ask 2: 101.0 (qty 2.0) - placed earlier at 101.0
    engine.process_message(OrderMessage::New {
        id: 2,
        timestamp: 200,
        account: 2,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 101.0,
        quantity: 2.0,
    });

    // Ask 3: 101.0 (qty 3.0) - placed later at 101.0
    engine.process_message(OrderMessage::New {
        id: 3,
        timestamp: 300,
        account: 3,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 101.0,
        quantity: 3.0,
    });

    // Incoming Buy for 4.0 @ 103.0
    // Should match Ask 2 first (2.0 @ 101.0), then Ask 3 (2.0 @ 101.0)
    let trades = engine.process_message(OrderMessage::New {
        id: 4,
        timestamp: 400,
        account: 4,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 103.0,
        quantity: 4.0,
    });

    assert_eq!(trades.len(), 2);
    assert_eq!(trades[0].sell_order_id, 2);
    assert_eq!(trades[0].quantity, 2.0);
    assert_eq!(trades[0].price, 101.0);

    assert_eq!(trades[1].sell_order_id, 3);
    assert_eq!(trades[1].quantity, 2.0);
    assert_eq!(trades[1].price, 101.0);

    // Book check: Ask 3 should have 1.0 remaining @ 101.0; Ask 1 has 2.0 @ 102.0
    let book = engine.get_book("ETH-USDC").unwrap();
    assert_eq!(book.asks.len(), 2);
    assert_eq!(book.best_ask, Some(101.0));
    assert_eq!(book.asks[0].quantity, 1.0);
    assert_eq!(book.asks[1].quantity, 2.0);
}

#[test]
fn test_cancellation() {
    let mut engine = MatchingEngine::new();

    // Place order
    engine.process_message(OrderMessage::New {
        id: 10,
        timestamp: 1000,
        account: 1,
        symbol: "BTC-USDC".to_string(),
        side: Side::Buy,
        price: 990.0,
        quantity: 1.0,
    });

    let book = engine.get_book("BTC-USDC").unwrap();
    assert_eq!(book.bids.len(), 1);

    // Cancel order
    engine.process_message(OrderMessage::Cancel {
        id: 11,
        timestamp: 1500,
        account: 1,
        target_id: 10,
    });

    let book = engine.get_book("BTC-USDC").unwrap();
    assert_eq!(book.bids.len(), 0);

    // Incoming sell should not match cancelled order
    let trades = engine.process_message(OrderMessage::New {
        id: 12,
        timestamp: 2000,
        account: 2,
        symbol: "BTC-USDC".to_string(),
        side: Side::Sell,
        price: 990.0,
        quantity: 1.0,
    });

    assert_eq!(trades.len(), 0);
}

#[test]
fn test_symbol_isolation() {
    let mut engine = MatchingEngine::new();

    // Buy order for ETH-USDC
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 100.0,
        quantity: 5.0,
    });

    // Sell order for BTC-USDC at same price
    let trades = engine.process_message(OrderMessage::New {
        id: 2,
        timestamp: 2000,
        account: 2,
        symbol: "BTC-USDC".to_string(),
        side: Side::Sell,
        price: 100.0,
        quantity: 5.0,
    });

    assert_eq!(trades.len(), 0);
}
