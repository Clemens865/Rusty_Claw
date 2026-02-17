//! Prometheus metrics recording and endpoint.

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Install the Prometheus metrics recorder and return the handle for rendering.
pub fn install_prometheus_recorder() -> PrometheusHandle {
    let builder = PrometheusBuilder::new();
    builder
        .install_recorder()
        .expect("Failed to install Prometheus recorder")
}

/// Record a new WebSocket connection.
pub fn record_ws_connect() {
    metrics::gauge!("ws_connections_active").increment(1.0);
}

/// Record a WebSocket disconnection.
pub fn record_ws_disconnect() {
    metrics::gauge!("ws_connections_active").decrement(1.0);
}

/// Record a WS method request with its duration.
pub fn record_request(method: &str, duration_secs: f64) {
    let labels = [("method", method.to_string())];
    metrics::counter!("ws_requests_total", &labels).increment(1);
    metrics::histogram!("ws_request_duration_seconds", &labels).record(duration_secs);
}

/// Record an agent run starting.
pub fn record_agent_start() {
    metrics::gauge!("agent_active").increment(1.0);
}

/// Record an agent run ending.
pub fn record_agent_end() {
    metrics::gauge!("agent_active").decrement(1.0);
}

/// Record an error of a given kind.
pub fn record_error(kind: &str) {
    let labels = [("kind", kind.to_string())];
    metrics::counter!("errors_total", &labels).increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_prometheus_recorder() {
        // Just verify it doesn't panic; can only install once per process
        // so we use a different approach: test the handle render
        let handle = install_prometheus_recorder();
        let output = handle.render();
        // Fresh recorder should have empty or minimal output
        assert!(output.is_empty() || output.contains("# "));
    }

    #[test]
    fn test_record_request_does_not_panic() {
        // These should not panic even without a recorder installed
        // (metrics crate uses a no-op recorder by default)
        record_request("test.method", 0.123);
    }

    #[test]
    fn test_record_agent_gauges_do_not_panic() {
        record_agent_start();
        record_agent_end();
    }

    #[test]
    fn test_record_error_does_not_panic() {
        record_error("test_error");
    }
}
