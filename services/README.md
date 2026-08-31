# Order Feed Service

This service provides a simple, local exchange order feed simulation with an API for interacting with it. You can start the feed, watch a continuous stream of random bid/ask limit orders (and occasional cancels) across several trading pairs, submit your own orders, and query recent messages.

The feed only *publishes* orders — it does not match them. What happens when a buy and a sell cross is entirely up to the consumers of the feed.

## Getting Started

[Install Rust](https://rust-lang.org/tools/install/) if you haven't yet.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Build the order feed in the `services` directory.

```bash
cd services
cargo build
```

Run the feed in the background or in its own terminal.

```bash
cargo run -- --start-feed --num-accounts 20
```

Learn more about available commands.

```bash
cargo run -- --help
```

## CLI Reference

### Starting the Feed

- `--start-feed`: Starts the order feed simulation server. This process will run continuously, publishing new order messages and listening for API requests on `http://127.0.0.1:3000`.

- `--num-accounts <NUMBER>`: Used in conjunction with `--start-feed` to specify how many simulated accounts place orders. If not provided, it defaults to 10.

- `--rate <NUMBER>`: Used in conjunction with `--start-feed` to specify roughly how many messages are published per second. If not provided, it defaults to 2.

### Interacting with the Feed

To interact with the running feed, you'll need to open a separate terminal.

- `--submit <ACCOUNT_ID> <SYMBOL> <SIDE> <PRICE> <QUANTITY>`: Submits a new limit order onto the feed.
    - `<ACCOUNT_ID>`: The account ID placing the order.
    - `<SYMBOL>`: The trading pair. Can be `NEX-USDC`, `ETH-USDC`, or `BTC-USDC`.
    - `<SIDE>`: `Buy` (bid) or `Sell` (ask).
    - `<PRICE>`: The limit price (e.g., `100.25`).
    - `<QUANTITY>`: The order quantity (e.g., `5.0`).

- `--cancel <ACCOUNT_ID> <ORDER_ID>`: Submits a cancel for a previously placed order.
    - `<ACCOUNT_ID>`: The account ID cancelling the order.
    - `<ORDER_ID>`: The `id` of the order to cancel.

- `--orders [NUMBER]`: Fetches and displays the most recent feed messages.
    - `[NUMBER]` (optional): The number of recent messages to fetch. If omitted, it defaults to 10.

**Examples:**

Submit a bid for 5 ETH-USDC at 100.25 from account 0:
```bash
cargo run -- --submit 0 ETH-USDC Buy 100.25 5
```

Cancel order 42 from account 0:
```bash
cargo run -- --cancel 0 42
```

Get the last 5 messages:
```bash
cargo run -- --orders 5
```

Get the last 10 messages (default):
```bash
cargo run -- --orders
```

## API Reference

### Interacting with the Feed via HTTP Requests

The feed server exposes a REST API on `http://127.0.0.1:3000` that allows you to consume the order stream and submit orders programmatically. Below are the available endpoints:

#### Get Feed Messages

**Endpoint:** `GET /orders`
**Query Parameters:**
- `since` (optional): Return only messages with an `id` strictly greater than this value. Message IDs are strictly increasing and start at 1, so you can consume the entire stream by polling with the highest `id` you have seen so far (start with `since=0`).
- `n` (optional): Return only the last `n` messages instead.

**Response:** JSON array of feed messages, in feed order.

**Message Types:**

1. **New Order** — a new limit order entering the market:
```json
{
  "New": {
    "id": 42,
    "timestamp": 1753000000000,
    "account": 3,
    "symbol": "ETH-USDC",
    "side": "Buy",
    "price": 100.25,
    "quantity": 5.0
  }
}
```

2. **Cancel** — a request to cancel a previously placed order (`target_id` refers to the `id` of the order to cancel):
```json
{
  "Cancel": {
    "id": 57,
    "timestamp": 1753000004000,
    "account": 3,
    "target_id": 42
  }
}
```

**Example with curl:**

Poll for all messages after id 42:
```bash
curl "http://127.0.0.1:3000/orders?since=42"
```

Get the last 5 messages:
```bash
curl "http://127.0.0.1:3000/orders?n=5"
```

#### Submit Order

**Endpoint:** `POST /order`
**Content-Type:** `application/json`

Submit a new limit order onto the feed. The order is assigned an `id` and published to all consumers like any other feed message.

```json
{
  "account": 0,
  "symbol": "ETH-USDC",
  "side": "Buy",
  "price": 100.25,
  "quantity": 5.0
}
```

**Response:** the `id` assigned to the order.

```json
{ "id": 123 }
```

**Example with curl:**

```bash
curl -X POST http://127.0.0.1:3000/order \
  -H "Content-Type: application/json" \
  -d '{
    "account": 0,
    "symbol": "ETH-USDC",
    "side": "Buy",
    "price": 100.25,
    "quantity": 5.0
  }'
```

#### Submit Cancel

**Endpoint:** `POST /cancel`
**Content-Type:** `application/json`

Submit a cancel onto the feed. The feed does not check whether the target order is still open — consumers of the feed decide what a cancel means for their own state.

```json
{
  "account": 0,
  "target_id": 42
}
```

**Example with curl:**

```bash
curl -X POST http://127.0.0.1:3000/cancel \
  -H "Content-Type: application/json" \
  -d '{ "account": 0, "target_id": 42 }'
```

#### Get Symbols

**Endpoint:** `GET /symbols`

**Response:** JSON array of available trading pairs.

```json
["NEX-USDC", "ETH-USDC", "BTC-USDC"]
```

**Available Symbols:**
- `NEX-USDC`
- `ETH-USDC`
- `BTC-USDC`

## Feed Behavior Notes

- Orders are random limit orders priced around a per-symbol mid price that drifts slowly as a random walk, so bids and asks frequently cross.
- Roughly 15% of generated messages are cancels of recent orders. A cancel may arrive for an order that (in your view of the market) has already traded — handling this is up to you.
- The feed is a raw stream: it does not track balances or positions, and it never reports whether orders matched. There are no trade/execution messages.

---

# Matching Engine & Market Data Service

The matching engine service consumes the order feed, maintains in-memory limit order books with price-time priority for all symbols, matches crossing buy and sell orders, executes trades, and handles cancellations.

It persists all market data (trade history, book snapshots, per-account positions, and notional cash) to a SQLite relational database and provides HTTP query endpoints and CLI commands.

## Starting the Matching Engine

Start the matching engine (connecting to the feed on port 3000):

```bash
cargo run -- --start-matcher
```

Or start both the feed and matching engine together in a single process:

```bash
cargo run -- --start-all
```

Custom options:
- `--feed-url <URL>`: URL of the feed service (default `http://127.0.0.1:3000`).
- `--matcher-port <PORT>`: HTTP API port for matching engine queries (default `3001`).
- `--poll-interval-ms <MS>`: Polling interval in ms (default `100`).
- `--db-path <PATH>`: Path to SQLite database file (default `market_data.db`, or use `:memory:`).

## Interacting with the Matching Engine & Market Data

### CLI Commands

- View order book (all symbols or specific symbol):
  ```bash
  cargo run -- --book
  cargo run -- --book ETH-USDC
  ```

- View executed trades (all symbols or specific symbol):
  ```bash
  cargo run -- --trades
  cargo run -- --trades ETH-USDC
  ```

- View account portfolio (positions, notional cash balance, and fill count):
  ```bash
  cargo run -- --account 0
  ```

- View all active accounts with positions and cash:
  ```bash
  cargo run -- --accounts
  ```

- View matching statistics:
  ```bash
  cargo run -- --matcher-stats
  ```

### HTTP API (`http://127.0.0.1:3001`)

- `GET /trades?symbol=ETH-USDC&account=0&limit=20`: List executed trades with optional symbol and account filter.
- `GET /book?symbol=ETH-USDC`: View order book depth, spread, and open orders.
- `GET /account?id=0`: View account's token positions, notional cash balance, and trade counts.
- `GET /accounts`: List all active account portfolios.
- `GET /stats`: View engine performance statistics (messages processed, trades executed, total volume).
- `GET /health`: Health check endpoint.

## SQLite Database Schema

The market data service stores data across the following tables in SQLite:
- `trades`: Record of all matched executions (`id`, `timestamp`, `symbol`, `buy_order_id`, `sell_order_id`, `maker_order_id`, `taker_order_id`, `buy_account`, `sell_account`, `price`, `quantity`, `value`).
- `account_positions`: Net asset holdings per account per symbol (`account_id`, `symbol`, `position`).
- `account_balances`: Notional cash balances in quote currency per account (`account_id`, `cash`).
- `book_snapshots`: Serialized book state and top-of-book prices per symbol (`symbol`, `best_bid`, `best_ask`, `spread`, `snapshot_json`, `updated_at`).
