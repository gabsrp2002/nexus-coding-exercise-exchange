use services::engine::MatchingEngine;
use services::feed::{OrderMessage, Side};

#[test]
fn test_db_trade_persistence_and_filtering() {
    let mut engine = MatchingEngine::new();

    // 1. Account 1 sells 5.0 ETH-USDC @ 100.0
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 100.0,
        quantity: 5.0,
    });

    // 2. Account 2 buys 5.0 ETH-USDC @ 100.0
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

    // 3. Account 3 sells 1.0 BTC-USDC @ 1000.0
    engine.process_message(OrderMessage::New {
        id: 3,
        timestamp: 3000,
        account: 3,
        symbol: "BTC-USDC".to_string(),
        side: Side::Sell,
        price: 1000.0,
        quantity: 1.0,
    });

    // 4. Account 1 buys 1.0 BTC-USDC @ 1000.0
    let btc_trades = engine.process_message(OrderMessage::New {
        id: 4,
        timestamp: 4000,
        account: 1,
        symbol: "BTC-USDC".to_string(),
        side: Side::Buy,
        price: 1000.0,
        quantity: 1.0,
    });
    assert_eq!(btc_trades.len(), 1);

    // Check all trades
    let all_trades = engine.get_trades(None, None, None);
    assert_eq!(all_trades.len(), 2);

    // Filter by symbol
    let eth_trades = engine.get_trades(Some("ETH-USDC"), None, None);
    assert_eq!(eth_trades.len(), 1);
    assert_eq!(eth_trades[0].symbol, "ETH-USDC");

    // Filter by account (Account 1 was seller in ETH trade and buyer in BTC trade)
    let acc1_trades = engine.get_trades(None, Some(1), None);
    assert_eq!(acc1_trades.len(), 2);

    // Filter by account 2 (only in ETH trade)
    let acc2_trades = engine.get_trades(None, Some(2), None);
    assert_eq!(acc2_trades.len(), 1);

    // Limit check
    let limited = engine.get_trades(None, None, Some(1));
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].id, 2); // Most recent trade
}

#[test]
fn test_db_account_positions_and_cash() {
    let mut engine = MatchingEngine::new();

    // Account 1 sells 5.0 ETH @ 100.0 to Account 2
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 100.0,
        quantity: 5.0,
    });

    engine.process_message(OrderMessage::New {
        id: 2,
        timestamp: 2000,
        account: 2,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 100.0,
        quantity: 5.0,
    });

    // Account 1: Seller receives $500.00 cash, gives 5.0 ETH
    let p1 = engine.get_account_portfolio(1).unwrap();
    assert_eq!(p1.account_id, 1);
    assert_eq!(p1.cash, 500.0);
    assert_eq!(p1.trades_count, 1);
    assert_eq!(p1.positions.len(), 1);
    assert_eq!(p1.positions[0].symbol, "ETH-USDC");
    assert_eq!(p1.positions[0].position, -5.0);

    // Account 2: Buyer pays $500.00 cash, receives 5.0 ETH
    let p2 = engine.get_account_portfolio(2).unwrap();
    assert_eq!(p2.account_id, 2);
    assert_eq!(p2.cash, -500.0);
    assert_eq!(p2.trades_count, 1);
    assert_eq!(p2.positions.len(), 1);
    assert_eq!(p2.positions[0].symbol, "ETH-USDC");
    assert_eq!(p2.positions[0].position, 5.0);
}

#[test]
fn test_db_multi_symbol_positions_netting() {
    let mut engine = MatchingEngine::new();

    // Account 1 buys 2.0 ETH @ 100.0 from Account 2 (Account 1: cash = -200, ETH = +2)
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 2,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 100.0,
        quantity: 2.0,
    });
    engine.process_message(OrderMessage::New {
        id: 2,
        timestamp: 2000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 100.0,
        quantity: 2.0,
    });

    // Account 1 sells 1.0 BTC @ 1000.0 to Account 3 (Account 1: cash = -200 + 1000 = +800, BTC = -1)
    engine.process_message(OrderMessage::New {
        id: 3,
        timestamp: 3000,
        account: 3,
        symbol: "BTC-USDC".to_string(),
        side: Side::Buy,
        price: 1000.0,
        quantity: 1.0,
    });
    engine.process_message(OrderMessage::New {
        id: 4,
        timestamp: 4000,
        account: 1,
        symbol: "BTC-USDC".to_string(),
        side: Side::Sell,
        price: 1000.0,
        quantity: 1.0,
    });

    // Account 1 buys another 1.0 ETH @ 120.0 from Account 4 (Account 1: cash = 800 - 120 = +680, ETH = +3)
    engine.process_message(OrderMessage::New {
        id: 5,
        timestamp: 5000,
        account: 4,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 120.0,
        quantity: 1.0,
    });
    engine.process_message(OrderMessage::New {
        id: 6,
        timestamp: 6000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 120.0,
        quantity: 1.0,
    });

    let p1 = engine.get_account_portfolio(1).unwrap();
    assert_eq!(p1.cash, 680.0);
    assert_eq!(p1.trades_count, 3);
    assert_eq!(p1.positions.len(), 2);

    let eth_pos = p1.positions.iter().find(|p| p.symbol == "ETH-USDC").unwrap();
    assert_eq!(eth_pos.position, 3.0);

    let btc_pos = p1.positions.iter().find(|p| p.symbol == "BTC-USDC").unwrap();
    assert_eq!(btc_pos.position, -1.0);

    // Verify all accounts listing
    let all_accounts = engine.get_all_accounts().unwrap();
    assert_eq!(all_accounts.len(), 4);

    // Verify zero-sum conservation of cash and tokens across all accounts
    let total_cash: f64 = all_accounts.iter().map(|a| a.cash).sum();
    assert!((total_cash).abs() < 1e-5, "Total cash must sum to zero across all accounts");

    let total_eth: f64 = all_accounts
        .iter()
        .flat_map(|a| a.positions.iter())
        .filter(|p| p.symbol == "ETH-USDC")
        .map(|p| p.position)
        .sum();
    assert!((total_eth).abs() < 1e-5, "Total ETH position must sum to zero across all accounts");

    let total_btc: f64 = all_accounts
        .iter()
        .flat_map(|a| a.positions.iter())
        .filter(|p| p.symbol == "BTC-USDC")
        .map(|p| p.position)
        .sum();
    assert!((total_btc).abs() < 1e-5, "Total BTC position must sum to zero across all accounts");
}

#[test]
fn test_db_book_snapshot_persistence() {
    let mut engine = MatchingEngine::new();

    // Place an ask order
    engine.process_message(OrderMessage::New {
        id: 1,
        timestamp: 1000,
        account: 1,
        symbol: "ETH-USDC".to_string(),
        side: Side::Sell,
        price: 102.5,
        quantity: 4.0,
    });

    // Place a bid order
    engine.process_message(OrderMessage::New {
        id: 2,
        timestamp: 2000,
        account: 2,
        symbol: "ETH-USDC".to_string(),
        side: Side::Buy,
        price: 100.0,
        quantity: 3.0,
    });

    // Check DB snapshot
    let snapshot = engine.db.get_book_snapshot("ETH-USDC").unwrap().unwrap();
    assert_eq!(snapshot.symbol, "ETH-USDC");
    assert_eq!(snapshot.best_bid, Some(100.0));
    assert_eq!(snapshot.best_ask, Some(102.5));
    assert_eq!(snapshot.spread, Some(2.5));
    assert_eq!(snapshot.bids.len(), 1);
    assert_eq!(snapshot.asks.len(), 1);
    assert_eq!(snapshot.open_orders.len(), 2);
}
