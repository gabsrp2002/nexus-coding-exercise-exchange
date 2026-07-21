use clap::Parser;
use serde_json::json;
use services::feed;
use std::str::FromStr;

/// Defines the command-line arguments for the application.
/// This structure uses `clap` to parse and validate arguments.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Flag to start the order feed simulation.
    #[arg(long)]
    start_feed: bool,

    /// The number of simulated accounts placing orders on the feed.
    #[arg(long, default_value_t = 10)]
    num_accounts: u32,

    /// The average number of feed messages generated per second.
    #[arg(long, default_value_t = 2.0)]
    rate: f64,

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
}

/// The entry point of the application.
/// It parses command-line arguments and executes the corresponding logic.
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    // The main logic flow is determined by which command-line argument is provided.
    // The application handles one primary command at a time.
    if let Some(submit_args) = args.submit {
        // This block handles the --submit command.
        // It parses the arguments and submits a new order to the feed.
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

        let client = reqwest::Client::new();
        let res = client
            .post("http://127.0.0.1:3000/order")
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
        // This block handles the --cancel command.
        // It parses the arguments and submits a cancel to the feed.
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

        let client = reqwest::Client::new();
        let res = client
            .post("http://127.0.0.1:3000/cancel")
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
        // This block handles the --orders command.
        // It fetches the n most recent feed messages and prints them.
        let n: usize = n_str.parse().expect("Invalid number of messages.");
        let client = reqwest::Client::new();
        let res = client
            .get(format!("http://127.0.0.1:3000/orders?n={}", n))
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
    } else if args.start_feed {
        // This block handles the --start-feed command.
        // It initializes and starts the order feed simulation.
        feed::start_feed(args.num_accounts, args.rate).await;
    } else {
        // If no command is provided, print a usage message.
        println!("Please specify a command, e.g., --start-feed, --submit, --cancel, or --orders.");
    }
    Ok(())
}
