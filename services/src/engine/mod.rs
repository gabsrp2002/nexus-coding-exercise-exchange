pub mod book;
pub mod consumer;
pub mod matcher;
pub mod server;
pub mod types;

pub use book::OrderBook;
pub use consumer::FeedConsumer;
pub use matcher::MatchingEngine;
pub use types::{BookOrder, EngineStats, OrderBookView, PriceLevelView, Trade};

use std::sync::{Arc, Mutex};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

/// Initializes and starts the matching engine service:
/// 1. Configures logging.
/// 2. Creates the shared MatchingEngine state.
/// 3. Spawns the background feed consumer.
/// 4. Runs the HTTP query server.
pub async fn start_engine(feed_url: String, port: u16, poll_interval_ms: u64) {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let engine = Arc::new(Mutex::new(MatchingEngine::new()));

    // Spawn the background feed polling consumer
    let consumer_engine = Arc::clone(&engine);
    let consumer_feed_url = feed_url.clone();
    tokio::spawn(async move {
        let mut consumer = FeedConsumer::new(consumer_feed_url, consumer_engine, poll_interval_ms);
        consumer.run().await;
    });

    // Run the HTTP API server
    server::run_server(engine, port).await;
}
