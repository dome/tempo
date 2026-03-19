use alloy::transports::http::reqwest::{self, Url};
use reth_tracing::tracing::{debug, info, warn};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

/// A single timestamped snapshot of Prometheus metrics from one node.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    /// Seconds since benchmark start.
    pub elapsed_secs: f64,
    /// Which metrics URL this came from.
    pub url: String,
    /// Parsed metric name → value (gauges and counters only).
    pub metrics: BTreeMap<String, f64>,
}

/// Collects metrics snapshots over time.
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    snapshots: Arc<Mutex<Vec<MetricsSnapshot>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn push(&self, snapshot: MetricsSnapshot) {
        self.snapshots.lock().unwrap().push(snapshot);
    }

    /// Consume the collector and return all snapshots.
    pub fn into_snapshots(self) -> Vec<MetricsSnapshot> {
        match Arc::try_unwrap(self.snapshots) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        }
    }
}

/// Parse Prometheus exposition format text into metric name → value pairs.
///
/// Only parses simple numeric lines (gauges, counters, untyped).
/// Skips comments, histograms buckets, and info metrics.
fn parse_prometheus_text(text: &str) -> BTreeMap<String, f64> {
    let mut metrics = BTreeMap::new();

    for line in text.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Format: metric_name{labels} value [timestamp]
        // or:     metric_name value [timestamp]
        let (name_part, rest) = if line.find('{').is_some() {
            // Has labels — use full name{labels} as key
            if let Some(brace_end) = line.find('}') {
                let key = &line[..=brace_end];
                let rest = line[brace_end + 1..].trim();
                (key, rest)
            } else {
                continue;
            }
        } else {
            // No labels — split on whitespace
            match line.split_once(|c: char| c.is_whitespace()) {
                Some((name, rest)) => (name, rest.trim()),
                None => continue,
            }
        };

        // Parse value (first token after name)
        let value_str = rest.split_whitespace().next().unwrap_or("");
        if let Ok(value) = value_str.parse::<f64>()
            && value.is_finite()
        {
            metrics.insert(name_part.to_string(), value);
        }
    }

    metrics
}

/// Spawn a background task that periodically scrapes Prometheus metrics.
///
/// Returns the collector handle and the join handle for the scraper task.
pub fn spawn_scraper(
    metrics_urls: Vec<Url>,
    scrape_interval: Duration,
    token: CancellationToken,
) -> MetricsCollector {
    let collector = MetricsCollector::new();
    let collector_clone = collector.clone();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to create metrics HTTP client");

    info!(
        urls = ?metrics_urls.iter().map(|u| u.as_str()).collect::<Vec<_>>(),
        interval_secs = scrape_interval.as_secs(),
        "Starting metrics scraper"
    );

    tokio::spawn(async move {
        let start = Instant::now();
        let mut ticker = tokio::time::interval(scrape_interval);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let elapsed = start.elapsed();

                    for url in &metrics_urls {
                        match client.get(url.clone()).send().await {
                            Ok(resp) => match resp.text().await {
                                Ok(body) => {
                                    let metrics = parse_prometheus_text(&body);
                                    debug!(
                                        url = %url,
                                        metric_count = metrics.len(),
                                        elapsed_secs = elapsed.as_secs(),
                                        "Scraped metrics"
                                    );
                                    collector_clone.push(MetricsSnapshot {
                                        elapsed_secs: elapsed.as_secs_f64(),
                                        url: url.to_string(),
                                        metrics,
                                    });
                                }
                                Err(e) => {
                                    warn!(url = %url, error = %e, "Failed to read metrics response body");
                                }
                            },
                            Err(e) => {
                                warn!(url = %url, error = %e, "Failed to scrape metrics");
                            }
                        }
                    }
                }
                _ = token.cancelled() => {
                    // Do one final scrape
                    let elapsed = start.elapsed();
                    for url in &metrics_urls {
                        if let Ok(resp) = client.get(url.clone()).send().await
                            && let Ok(body) = resp.text().await {
                                let metrics = parse_prometheus_text(&body);
                                collector_clone.push(MetricsSnapshot {
                                    elapsed_secs: elapsed.as_secs_f64(),
                                    url: url.to_string(),
                                    metrics,
                                });
                            }
                    }
                    break;
                }
            }
        }

        info!(
            total_snapshots = collector_clone.snapshots.lock().unwrap().len(),
            "Metrics scraper stopped"
        );
    });

    collector
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_metrics() {
        let text = r#"
# HELP reth_block_time Block time in milliseconds
# TYPE reth_block_time gauge
reth_block_time 250
reth_txpool_pending 1234
reth_process_cpu_seconds_total 42.5
"#;
        let metrics = parse_prometheus_text(text);
        assert_eq!(metrics["reth_block_time"], 250.0);
        assert_eq!(metrics["reth_txpool_pending"], 1234.0);
        assert_eq!(metrics["reth_process_cpu_seconds_total"], 42.5);
    }

    #[test]
    fn test_parse_labeled_metrics() {
        let text = r#"
http_requests_total{method="GET",path="/api"} 100
http_requests_total{method="POST",path="/api"} 50
"#;
        let metrics = parse_prometheus_text(text);
        assert_eq!(
            metrics[r#"http_requests_total{method="GET",path="/api"}"#],
            100.0
        );
        assert_eq!(
            metrics[r#"http_requests_total{method="POST",path="/api"}"#],
            50.0
        );
    }

    #[test]
    fn test_parse_skips_invalid() {
        let text = r#"
# comment
valid_metric 42

invalid_line
metric_with_nan NaN
metric_with_inf +Inf
"#;
        let metrics = parse_prometheus_text(text);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics["valid_metric"], 42.0);
    }
}
