use clap::{ArgGroup, Parser};
use serde_json::json;
use services::engine;
use services::feed;
use std::str::FromStr;

/// Defines the command-line arguments for the application.
/// This structure uses `clap` to parse and validate arguments.
/// The primary commands are mutually exclusive; providing more than one
/// results in a clear error instead of a silent skip.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(group(
    ArgGroup::new("command").args([
        "start_feed",
        "start_matcher",
        "start_all",
        "submit",
        "cancel",
        "orders",
        "book",
        "trades",
        "matcher_stats",
    ]),
))]
struct Args {
    /// Flag to start the order feed simulation.
    #[arg(long)]
    start_feed: bool,

    /// Flag to start the matching engine service.
    #[arg(long)]
    start_matcher: bool,

    /// Flag to start both the order feed simulation and the matching engine service together.
    #[arg(long)]
    start_all: bool,

    /// The number of simulated accounts placing orders on the feed.
    #[arg(long, default_value_t = 10)]
    num_accounts: u32,

    /// The average number of feed messages generated per second.
    #[arg(long, default_value_t = 2.0)]
    rate: f64,

    /// The URL of the order feed service to connect to.
    #[arg(long, default_value = "http://127.0.0.1:3000")]
    feed_url: String,

    /// The port for the matching engine HTTP server.
    #[arg(long, default_value_t = 3001)]
    matcher_port: u16,

    /// The polling interval in milliseconds for the feed consumer.
    #[arg(long, default_value_t = 100)]
    poll_interval_ms: u64,

    /// The arguments for submitting a new order.
    /// Expects account ID, symbol, side, price, and quantity.
    #[arg(long, num_args = 5, value_names = ["ACCOUNT_ID", "SYMBOL", "SIDE", "PRICE", "QUANTITY"])]
    submit: Option<Vec<String>>,

    /// The arguments for submitting a cancel.
    /// Expects account ID and the ID of the order to cancel.
    #[arg(long, num_args = 2, value_names = ["ACCOUNT_ID", "ORDER_ID"])]
    cancel: Option<Vec<String>>,

    /// Fetches and prints the most recent n feed messages.
    /// Defaults to 10 if no number is provided.
    #[arg(long, num_args = 0..=1, default_missing_value = "10")]
    orders: Option<String>,

    /// Fetches and prints the current order book from the matching engine.
    /// Optionally specify a symbol (e.g. --book ETH-USDC).
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    book: Option<String>,

    /// Fetches and prints recent executed trades from the matching engine.
    /// Optionally specify a symbol (e.g. --trades ETH-USDC).
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    trades: Option<String>,

    /// Fetches and prints statistics from the matching engine.
    #[arg(long)]
    matcher_stats: bool,
}

/// The entry point of the application.
/// It parses command-line arguments and executes the corresponding logic.
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    if let Some(submit_args) = args.submit {
        // Submit an order to the feed
        let account: u32 = submit_args[0]
            .parse()
            .expect("Invalid account ID. Must be a number.");
        let symbol = submit_args[1].to_uppercase();
        let side = feed::Side::from_str(&submit_args[2]).expect("Invalid side specified.");
        let price: f64 = submit_args[3]
            .parse()
            .expect("Invalid price specified. Must be a number.");
        let quantity: f64 = submit_args[4]
            .parse()
            .expect("Invalid quantity specified. Must be a number.");

        let body = json!({
            "account": account,
            "symbol": symbol,
            "side": side,
            "price": price,
            "quantity": quantity,
        });

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let res = client
            .post(format!("{}/order", args.feed_url.trim_end_matches('/')))
            .json(&body)
            .send()
            .await
            .expect("Failed to submit order to the feed.");

        if res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            println!("Order submitted to feed successfully: {}", text);
        } else {
            println!("Failed to submit order. Status: {}", res.status());
        }
    } else if let Some(cancel_args) = args.cancel {
        // Submit a cancel to the feed
        let account: u32 = cancel_args[0]
            .parse()
            .expect("Invalid account ID. Must be a number.");
        let target_id: u64 = cancel_args[1]
            .parse()
            .expect("Invalid order ID. Must be a number.");

        let body = json!({
            "account": account,
            "target_id": target_id,
        });

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let res = client
            .post(format!("{}/cancel", args.feed_url.trim_end_matches('/')))
            .json(&body)
            .send()
            .await
            .expect("Failed to submit cancel to the feed.");

        if res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            println!("Cancel submitted to feed successfully: {}", text);
        } else {
            println!("Failed to submit cancel. Status: {}", res.status());
        }
    } else if let Some(n_str) = args.orders {
        // Fetch recent messages from feed
        let n: usize = n_str.parse().expect("Invalid number of messages.");
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let res = client
            .get(format!(
                "{}/orders?n={}",
                args.feed_url.trim_end_matches('/'),
                n
            ))
            .send()
            .await
            .expect("Failed to get messages from the feed.");
        if res.status().is_success() {
            let messages: Vec<feed::OrderMessage> = res
                .json()
                .await
                .expect("Failed to parse messages from response.");
            println!("Most recent {} messages:", messages.len());
            println!("{:#?}", messages);
        } else {
            println!("Failed to get messages. Status: {}", res.status());
        }
    } else if let Some(sym_str) = args.book {
        // Fetch order book from matching engine
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = if sym_str.trim().is_empty() {
            format!("http://127.0.0.1:{}/book", args.matcher_port)
        } else {
            format!(
                "http://127.0.0.1:{}/book?symbol={}",
                args.matcher_port,
                sym_str.trim().to_uppercase()
            )
        };
        let res = client
            .get(&url)
            .send()
            .await
            .expect("Failed to connect to matching engine. Is --start-matcher running?");
        if res.status().is_success() {
            let book_json: serde_json::Value = res.json().await.unwrap();
            println!("{}", serde_json::to_string_pretty(&book_json).unwrap());
        } else {
            println!("Failed to fetch order book. Status: {}", res.status());
        }
    } else if let Some(sym_str) = args.trades {
        // Fetch trades from matching engine
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = if sym_str.trim().is_empty() {
            format!("http://127.0.0.1:{}/trades", args.matcher_port)
        } else {
            format!(
                "http://127.0.0.1:{}/trades?symbol={}",
                args.matcher_port,
                sym_str.trim().to_uppercase()
            )
        };
        let res = client
            .get(&url)
            .send()
            .await
            .expect("Failed to connect to matching engine. Is --start-matcher running?");
        if res.status().is_success() {
            let trades_json: serde_json::Value = res.json().await.unwrap();
            println!("{}", serde_json::to_string_pretty(&trades_json).unwrap());
        } else {
            println!("Failed to fetch trades. Status: {}", res.status());
        }
    } else if args.matcher_stats {
        // Fetch matching engine stats
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = format!("http://127.0.0.1:{}/stats", args.matcher_port);
        let res = client
            .get(&url)
            .send()
            .await
            .expect("Failed to connect to matching engine. Is --start-matcher running?");
        if res.status().is_success() {
            let stats_json: serde_json::Value = res.json().await.unwrap();
            println!("{}", serde_json::to_string_pretty(&stats_json).unwrap());
        } else {
            println!("Failed to fetch stats. Status: {}", res.status());
        }
    } else if args.start_feed {
        // Start the order feed simulation
        feed::start_feed(args.num_accounts, args.rate).await;
    } else if args.start_matcher {
        // Start the matching engine & consumer
        engine::start_engine(args.feed_url, args.matcher_port, args.poll_interval_ms).await;
    } else if args.start_all {
        // Start both the order feed and the matching engine
        let num_accounts = args.num_accounts;
        let rate = args.rate;
        tokio::spawn(async move {
            feed::start_feed(num_accounts, rate).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        engine::start_engine(args.feed_url, args.matcher_port, args.poll_interval_ms).await;
    } else {
        println!(
            "Please specify a command:\n  --start-feed: Start feed generator\n  --start-matcher: Start matching engine service\n  --start-all: Start feed & matching engine together\n  --submit, --cancel, --orders\n  --book [SYMBOL], --trades [SYMBOL], --matcher-stats"
        );
    }
    Ok(())
}
