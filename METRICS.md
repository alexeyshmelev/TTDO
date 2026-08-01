# Metrics

This document describes the metrics exposed by the TrustTunnel endpoint for
monitoring and health checks.

## Overview

The endpoint exposes Prometheus-compatible metrics through a pull-only HTTP
listener. The listener is disabled unless the optional `[metrics]` table is
present in `vpn.toml`.

## Configuration

Enable the listener in the endpoint's `vpn.toml`:

```toml
[metrics]
address = "127.0.0.1:1987"
request_timeout_secs = 3
```

Restart the endpoint after changing `vpn.toml`. With the values above, scrape
`http://127.0.0.1:1987/metrics` and probe
`http://127.0.0.1:1987/health-check` from the VPS itself.

The listener has no authentication or TLS. Keep it on loopback and use a
trusted local collector, or place an authenticated reverse proxy in front of
it. Do not publish it directly to the Internet.

Library integrations use `trusttunnel::settings::MetricsSettings::builder()`.
Set the address with `listen_address`, set a `Duration` with
`request_timeout`, call `build`, and pass the result to
`trusttunnel::settings::Settings::builder().metrics(...)`. The
`MetricsSettings` fields are private; constructing the struct with a literal is
not part of the public API.

## Endpoints

### `/metrics`

Returns all metrics in Prometheus text format.

**Example response:**

```console
# HELP client_sessions Number of active client sessions
# TYPE client_sessions gauge
client_sessions{protocol_type="http1"} 5
client_sessions{protocol_type="http2"} 3

# HELP inbound_traffic_bytes Total number of bytes uploaded by clients
# TYPE inbound_traffic_bytes counter
inbound_traffic_bytes{protocol_type="http1"} 1234567

# HELP outbound_traffic_bytes Total number of bytes downloaded by clients
# TYPE outbound_traffic_bytes counter
outbound_traffic_bytes{protocol_type="http1"} 7654321

# HELP outbound_tcp_sockets Number of active outbound TCP connections
# TYPE outbound_tcp_sockets gauge
outbound_tcp_sockets 12

# HELP outbound_udp_sockets Number of active outbound UDP sockets
# TYPE outbound_udp_sockets gauge
outbound_udp_sockets 8
```

### `/health-check`

Health check endpoint that returns HTTP 200 OK if the endpoint is running.

## Available Metrics

### Client Sessions

**Name:** `client_sessions`
**Type:** Gauge
**Labels:**

- `protocol_type`: Protocol type (`http1`, `http2`, `http3`)

**Description:** Current number of active client sessions grouped by protocol type.

**Use cases:**

- Monitor active connections
- Detect protocol distribution
- Identify connection leaks
- Capacity planning

### Inbound Traffic

**Name:** `inbound_traffic_bytes`
**Type:** Counter
**Labels:**

- `protocol_type`: Protocol type (`http1`, `http2`, `http3`)

**Description:** Total number of bytes uploaded by clients (client → endpoint → destination).

**Use cases:**

- Monitor upload bandwidth usage
- Track traffic patterns by protocol
- Billing and quota management
- Anomaly detection

### Outbound Traffic

**Name:** `outbound_traffic_bytes`
**Type:** Counter
**Labels:**

- `protocol_type`: Protocol type (`http1`, `http2`, `http3`)

**Description:** Total number of bytes downloaded by clients (destination → endpoint → client).

**Use cases:**

- Monitor download bandwidth usage
- Track traffic patterns by protocol
- Billing and quota management
- Anomaly detection

### Outbound TCP Sockets

**Name:** `outbound_tcp_sockets`
**Type:** Gauge
**Labels:** None

**Description:** Current number of active outbound TCP connections from the endpoint to destination servers.

**Use cases:**

- Monitor connection pool size
- Detect connection leaks
- Identify resource exhaustion
- Optimize connection limits
- Debug proxy performance issues

**Notes:**

- Incremented when a new TCP connection is established
- Decremented when the connection is closed
- Includes connections through direct forwarder and SOCKS5 forwarder
- Does not include connections to SOCKS5 proxy itself

### Outbound UDP Sockets

**Name:** `outbound_udp_sockets`
**Type:** Gauge
**Labels:** None

**Description:** Current number of active outbound UDP sockets from the endpoint to destination servers.

**Use cases:**

- Monitor UDP multiplexer state
- Detect socket leaks
- Track UDP traffic load
- Optimize socket pool configuration
- Debug UDP forwarding issues

**Notes:**

- Incremented when a new UDP association is created
- Decremented when the association is closed
- Includes sockets through direct forwarder and SOCKS5 UDP associations
- Each unique source-destination pair counts as one socket

## Metric Types

### Gauge

A gauge is a metric that represents a single numerical value that can arbitrarily go up and down. Gauges are typically used for measured values like current memory usage or number of active connections.

**Examples:** `client_sessions`, `outbound_tcp_sockets`, `outbound_udp_sockets`

### Counter

A counter is a cumulative metric that represents a single monotonically increasing counter whose value can only increase or be reset to zero. Counters are typically used for counts of events like number of requests or bytes transferred.

**Examples:** `inbound_traffic_bytes`, `outbound_traffic_bytes`

## Implementation Details

### Lifecycle Management

Metrics are automatically managed through RAII (Resource Acquisition Is Initialization) pattern:

- **Client sessions:** Counter incremented when session starts, decremented when session ends
- **TCP sockets:** Counter incremented when TCP connection established, decremented when closed
- **UDP sockets:** Counter incremented when UDP association created, decremented when cleaned up
- **Traffic counters:** Incremented as data flows through the pipe

## Troubleshooting

### Metrics endpoint not responding

1. Verify metrics are enabled in configuration
2. Check the listen address is not already in use
3. Verify firewall rules allow connections
4. Check logs for bind errors

### Missing metrics

1. Ensure client sessions are active to generate traffic metrics
2. Verify protocol type labels match expected values
3. Check metrics collection interval in your monitoring system

### Unexpected metric values

1. **Outbound sockets > client sessions:** Normal for HTTP/1.1 with multiple concurrent requests
2. **Outbound sockets = 0 with active sessions:** May indicate all requests are cached or failing
3. **Continuously growing sockets:** Check for connection leaks or slow destinations

## See Also

- [CONFIGURATION.md](CONFIGURATION.md) - Endpoint configuration reference
- [PROTOCOL.md](PROTOCOL.md) - Supported protocols documentation
- [DEVELOPMENT.md](DEVELOPMENT.md) - Development and testing guide
