use crate::engine::matcher::MatchingEngine;
use crate::feed::OrderMessage;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Background consumer that polls order messages from the feed service
/// and forwards them to the matching engine.
pub struct FeedConsumer {
    feed_url: String,
    engine: Arc<Mutex<MatchingEngine>>,
    poll_interval: Duration,
    last_seen_id: u64,
}

impl FeedConsumer {
    pub fn new(
        feed_url: String,
        engine: Arc<Mutex<MatchingEngine>>,
        poll_interval_ms: u64,
    ) -> Self {
        Self {
            feed_url,
            engine,
            poll_interval: Duration::from_millis(poll_interval_ms),
            last_seen_id: 0,
        }
    }

    /// Starts the consumer loop. Runs indefinitely until cancelled.
    pub async fn run(&mut self) {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        let orders_url = format!("{}/orders", self.feed_url.trim_end_matches('/'));
        info!(
            "Starting Feed Consumer polling from {} every {:?}",
            orders_url, self.poll_interval
        );

        let mut failure_count: u32 = 0;
        let mut last_log_time = Instant::now() - Duration::from_secs(10);
        let log_interval = Duration::from_secs(5);

        loop {
            let poll_url = format!("{}?since={}", orders_url, self.last_seen_id);

            match client.get(&poll_url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        if failure_count > 0 {
                            info!(
                                "✅ Successfully connected to feed at {} after {} failed attempts",
                                orders_url, failure_count
                            );
                            failure_count = 0;
                        }
                        match response.json::<Vec<OrderMessage>>().await {
                            Ok(messages) => {
                                if !messages.is_empty() {
                                    let mut engine = self.engine.lock().unwrap();
                                    for msg in messages {
                                        let msg_id = msg.id();
                                        engine.process_message(msg);
                                        if msg_id > self.last_seen_id {
                                            self.last_seen_id = msg_id;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to deserialize messages from feed: {}", e);
                            }
                        }
                    } else {
                        failure_count += 1;
                        if last_log_time.elapsed() >= log_interval {
                            let text = response.text().await.unwrap_or_default();
                            warn!(
                                "Feed server returned non-success HTTP status: {} | Response body: '{}' (Failures: {})",
                                status, text, failure_count
                            );
                            last_log_time = Instant::now();
                        }
                    }
                }
                Err(e) => {
                    failure_count += 1;
                    if last_log_time.elapsed() >= log_interval {
                        let mut causes = Vec::new();
                        let mut cur: Option<&(dyn Error + 'static)> = e.source();
                        while let Some(src) = cur {
                            causes.push(src.to_string());
                            cur = src.source();
                        }

                        warn!(
                            "Could not connect to feed service at {} (attempt #{}):\n  Error: {}\n  Causes: {}\n  Debug: {:?}",
                            poll_url,
                            failure_count,
                            e,
                            if causes.is_empty() {
                                "none".to_string()
                            } else {
                                causes.join(" -> ")
                            },
                            e
                        );
                        last_log_time = Instant::now();
                    }
                }
            }

            sleep(self.poll_interval).await;
        }
    }
}
