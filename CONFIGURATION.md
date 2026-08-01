# TrustTunnel Endpoint Configuration

This document describes all available configuration settings and configuration files for the TrustTunnel VPN endpoint.

## Table of Contents

- [Overview](#overview)
- [Command Line Arguments](#command-line-arguments)
- [Configuration Files](#configuration-files)
    - [Main Settings File (vpn.toml)](#main-settings-file-vpntoml)
    - [TLS Hosts Settings File (hosts.toml)](#tls-hosts-settings-file-hoststoml)
    - [Credentials File (credentials.toml)](#credentials-file-credentialstoml)
    - [Rules File (rules.toml)](#rules-file-rulestoml)
- [Settings Reference](#settings-reference)
    - [Core Settings](#core-settings)
    - [Listen Protocol Settings](#listen-protocol-settings)
    - [Forward Protocol Settings](#forward-protocol-settings)
    - [Reverse Proxy Settings](#reverse-proxy-settings)
    - [ICMP Settings](#icmp-settings)
    - [Metrics Settings](#metrics-settings)
- [TLS Hosts Reference](#tls-hosts-reference)
- [Rules Reference](#rules-reference)
- [Runtime Configuration](#runtime-configuration)

---

## Overview

The TrustTunnel endpoint uses TOML-formatted configuration files. It does not
use a database or remote configuration service. Configuration is split into:

1. **Main settings file** - Core endpoint configuration (timeouts, protocols, etc.)
2. **TLS hosts settings file** - TLS certificate and hostname configuration
3. **Credentials file** - Client authentication credentials
4. **Rules file** - Connection filtering rules

The `setup_wizard` tool can generate these files interactively. Treat the
credentials file, private keys, and exported client configurations as secrets;
never commit or upload them. Relative paths are resolved from the endpoint's
working directory.

---

## Command Line Arguments

The endpoint binary accepts the following command line arguments:

| Argument | Short | Description | Default |
| -------- | ------- | ----------- | ------- |
| `--version` | `-v` | Print version and exit | - |
| `--loglvl` | `-l` | Logging level (`info`, `debug`, `trace`) | `info` |
| `--logfile` | - | File path for storing logs (stdout if not specified) | stdout |
| `--jobs` | - | Number of worker threads (defaults to CPU count) | CPU count |
| `<settings>` | - | **Required.** Path to main settings file | - |
| `<tls_hosts_settings>` | - | **Required.** Path to TLS hosts settings file | - |
| `--client_config` | `-c` | Print endpoint config for specified client and exit | - |
| `--address` | `-a` | Endpoint address to add to client config (repeatable; requires `-c`). Accepts `ip`, `ip:port`, `domain`, or `domain:port`. | - |
| `--custom-sni` | `-s` | TLS SNI override for the client; must match a configured hostname or `allowed_sni` (requires `-c`) | - |
| `--client-random-prefix` | `-r` | Use an explicit `client_random_prefix` in the exported client config (requires `-c`). | - |
| `--generate-client-random-prefix` | - | Generate a new `client_random_prefix`, append a matching allow rule to `rules.toml`, and use it in the exported client config (requires `-c`). | - |
| `--prefix-length` | - | Length in bytes for generated `client_random_prefix` values (requires `--generate-client-random-prefix`). | `4` |
| `--prefix-percent` | - | Percentage of one bits in the generated mask (requires `--generate-client-random-prefix`). | `70` |
| `--prefix-mask` | - | Explicit hex mask for generated `client_random_prefix` values (requires `--generate-client-random-prefix`). Conflicts with `--prefix-length` and `--prefix-percent`. | - |
| `--format` | `-f` | Client output format: `deeplink` or `toml` (requires `-c`) | `deeplink` |
| `--name` | `-n` | Human-readable server name in the exported client configuration (requires `-c`) | - |
| `--dns-upstream` | `-d` | DNS upstream in the exported client configuration; repeatable (requires `-c`) | - |

### Examples

```bash
# Start the endpoint
./trusttunnel_endpoint vpn.toml hosts.toml

# Start with debug logging
./trusttunnel_endpoint vpn.toml hosts.toml -l debug

# Start with file logging
./trusttunnel_endpoint vpn.toml hosts.toml --logfile /var/log/trusttunnel.log

# Export client configuration with IP address
./trusttunnel_endpoint vpn.toml hosts.toml -c username -a 203.0.113.1

# Export client configuration with explicit port
./trusttunnel_endpoint vpn.toml hosts.toml -c username -a 203.0.113.1:443

# Export client configuration with domain name
./trusttunnel_endpoint vpn.toml hosts.toml -c username -a vpn.example.com

# Export client configuration with domain name and explicit port
./trusttunnel_endpoint vpn.toml hosts.toml -c username -a vpn.example.com:443

# Export private TOML for the graphical client
umask 077
./trusttunnel_endpoint vpn.toml hosts.toml -c username \
    -a vpn.example.com:443 --format toml > client.toml

# Export client configuration with an explicit client_random_prefix
./trusttunnel_endpoint vpn.toml hosts.toml -c username -a vpn.example.com \
    --client-random-prefix a0b0/f0f0

# Generate a new client_random_prefix, append an allow rule to rules.toml, and export it
./trusttunnel_endpoint vpn.toml hosts.toml -c username -a vpn.example.com \
    --generate-client-random-prefix

# Generate a new client_random_prefix with a custom mask
./trusttunnel_endpoint vpn.toml hosts.toml -c username -a vpn.example.com \
    --generate-client-random-prefix --prefix-mask aaaa7777
```

Both output formats contain client credentials. The endpoint prints them only
to local stdout; it does not send them to a QR or configuration website. Store
redirected output with mode `0600` and transfer it privately.

---

## Configuration Files

### Main Settings File (vpn.toml)

The main settings file contains core endpoint configuration. Example:

> Native deployments commonly use `listen_address = "0.0.0.0:443"`.
> If you run Docker with host-to-container mapping `443:8443`, use
> `listen_address = "0.0.0.0:8443"` inside `vpn.toml`.

```toml
# The address to listen on
listen_address = "0.0.0.0:443"

# Advertise IPv6 only when the VPS has working outbound IPv6
ipv6_available = false

# Whether connections to private network of the endpoint are allowed
allow_private_network_connections = false

# Timeout of an incoming TLS handshake (seconds)
tls_handshake_timeout_secs = 10

# Timeout of a client listener (seconds)
client_listener_timeout_secs = 600

# Timeout of outgoing connection establishment (seconds)
connection_establishment_timeout_secs = 30

# Idle timeout of tunneled TCP connections (seconds)
tcp_connections_timeout_secs = 604800

# Timeout of tunneled UDP "connections" (seconds)
udp_connections_timeout_secs = 300

# Optional global per-credential connection limits
# default_max_http2_conns_per_client = 16
# default_max_http3_conns_per_client = 2

# Path to credentials file
credentials_file = "credentials.toml"

# Path to rules file (optional)
rules_file = "rules.toml"

# Listen protocol settings
[listen_protocols]

[listen_protocols.http1]
upload_buffer_size = 32768

[listen_protocols.http2]
initial_connection_window_size = 8388608
initial_stream_window_size = 131072
max_concurrent_streams = 1000
max_frame_size = 16384
header_table_size = 65536

[listen_protocols.quic]
recv_udp_payload_size = 1350
send_udp_payload_size = 1350
initial_max_data = 104857600
initial_max_stream_data_bidi_local = 1048576
initial_max_stream_data_bidi_remote = 1048576
initial_max_stream_data_uni = 1048576
initial_max_streams_bidi = 4096
initial_max_streams_uni = 4096
max_connection_window = 25165824
max_stream_window = 16777216
disable_active_migration = true
enable_early_data = true
message_queue_capacity = 4096

# Forward protocol (optional, defaults to direct)
[forward_protocol]
direct = {}

# Reverse proxy settings (optional)
# [reverse_proxy]
# server_address = "127.0.0.1:8080"
# path_mask = "/api"
# h3_backward_compatibility = false

# ICMP settings (optional, requires superuser)
# [icmp]
# interface_name = "eth0"
# request_timeout_secs = 3
# recv_message_queue_capacity = 256

# Metrics settings (optional)
# [metrics]
# address = "127.0.0.1:1987"
# request_timeout_secs = 3
```

### TLS Hosts Settings File (hosts.toml)

Configures TLS certificates and hostnames. Example:

```toml
# Main TLS hosts for traffic tunneling
[[main_hosts]]
hostname = "vpn.example.com"
cert_chain_path = "certs/cert.pem"
private_key_path = "certs/key.pem"
# allowed_sni = ["alternate.example.com"]

# Ping hosts for HTTPS health checks (optional)
[[ping_hosts]]
hostname = "ping.vpn.example.com"
cert_chain_path = "certs/cert.pem"
private_key_path = "certs/key.pem"

# Speed test hosts (optional)
[[speedtest_hosts]]
hostname = "speed.vpn.example.com"
cert_chain_path = "certs/cert.pem"
private_key_path = "certs/key.pem"

# Reverse proxy hosts (optional, requires reverse_proxy in main settings)
# [[reverse_proxy_hosts]]
# hostname = "api.example.com"
# cert_chain_path = "certs/cert.pem"
# private_key_path = "certs/key.pem"
```

`allowed_sni` is optional. It lets a main host accept an additional SNI while
using that host's certificate and settings. Every value must still be valid for
the certificate presented to the client. Prefer one matching hostname for a
simple deployment.

### Credentials File (credentials.toml)

Contains client authentication credentials. Example:

```toml
[[client]]
username = "user1"
password = "secure_password_1"
max_http2_conns = 16
max_http3_conns = 2

[[client]]
username = "user2"
password = "secure_password_2"
```

The two connection limits are optional per-client overrides. HTTP/1.1 and
HTTP/2 share `max_http2_conns`; HTTP/3 uses `max_http3_conns`. If an override
and its global default are both absent, that protocol has no credential-level
connection-count limit.

Use unique random credentials and mode `0640` or stricter. If
`credentials_file` is omitted, the endpoint has no authenticator; do not expose
such a development configuration to the Internet. Changes are loaded only
after an endpoint restart.

### Rules File (rules.toml)

Defines connection filtering rules. Example:

```toml
# Rules are evaluated in order, first matching rule's action is applied.
# If no rules match, the connection is allowed by default.

# Deny connections from specific IP range
[[rule]]
cidr = "192.168.1.0/24"
action = "deny"

# Allow connections with specific TLS client random prefix
[[rule]]
client_random_prefix = "aabbcc"
action = "allow"

# Deny connections matching both IP and client random with mask
[[rule]]
cidr = "10.0.0.0/8"
client_random_prefix = "a0b0/f0f0"
action = "deny"
```

---

## Settings Reference

### Core Settings

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `listen_address` | String | `0.0.0.0:443` | Address and port to listen on |
| `ipv6_available` | Boolean | `true` | Advertise IPv6; enable resolved IPv6 targets and ICMPv6 |
| `allow_private_network_connections` | Boolean | `false` | Allow connections to endpoint's private network |
| `tls_handshake_timeout_secs` | Integer | `10` | TLS handshake timeout in seconds |
| `client_listener_timeout_secs` | Integer | `600` | Client listener timeout in seconds (10 minutes) |
| `connection_establishment_timeout_secs` | Integer | `30` | Outgoing connection timeout in seconds |
| `tcp_connections_timeout_secs` | Integer | `604800` | Idle TCP connection timeout (1 week) |
| `udp_connections_timeout_secs` | Integer | `300` | UDP connection timeout (5 minutes) |
| `credentials_file` | String | - | Path to credentials file |
| `rules_file` | String | - | Path to rules file (optional) |
| `speedtest_enable` | Boolean | `false` | Enable speedtest handler on main hosts |
| `ping_enable` | Boolean | `false` | Enable ping handler on main hosts |
| `ping_path` | String | - | Optional path prefix for ping on main hosts |
| `speedtest_path` | String | - | Optional path prefix for speedtest on main hosts |
| `auth_failure_status_code` | Integer | `407` | HTTP status code on auth failure for CONNECT requests |
| `non_connect_auth_failure_status_code` | Integer | - | HTTP status code on auth failure for non-CONNECT requests |
| `default_max_http2_conns_per_client` | Integer | - | Default simultaneous HTTP/1.1 and HTTP/2 connections per credential |
| `default_max_http3_conns_per_client` | Integer | - | Default simultaneous HTTP/3 connections per credential |

Ping and speedtest are matched only via their configured paths. Default paths are: `/ping` and `/speedtest`.
`auth_failure_status_code` and `non_connect_auth_failure_status_code` accept `407`, `405`, `404`, or `403`.
If `non_connect_auth_failure_status_code` is not set, it falls back to `auth_failure_status_code`.
Warning: using a value other than `407` for `auth_failure_status_code` breaks proxy authentication in Chrome.

Set `ipv6_available` to `false` unless the endpoint host has working outbound
IPv6. Exported client configurations use this value, so regenerate and re-import
them after changing it.

### Listen Protocol Settings

Configure which protocols the endpoint accepts. At least one protocol must be enabled.

#### HTTP/1.1 Settings (`[listen_protocols.http1]`)

| Setting | Type | Default | Description |
| ------- | ---- | ------- | ----------- |
| `upload_buffer_size` | Integer | `32768` | Buffer size for outgoing traffic (bytes) |

#### HTTP/2 Settings (`[listen_protocols.http2]`)

| Setting | Type | Default | Description |
| ------- | ---- | ------- | ----------- |
| `initial_connection_window_size` | Integer | `8388608` | Connection-level flow control window (8 MB) |
| `initial_stream_window_size` | Integer | `131072` | Stream-level flow control window (128 KB) |
| `max_concurrent_streams` | Integer | `1000` | Maximum concurrent streams |
| `max_frame_size` | Integer | `16384` | Maximum HTTP/2 frame payload size |
| `header_table_size` | Integer | `65536` | Maximum header frame size |

#### QUIC/HTTP/3 Settings (`[listen_protocols.quic]`)

| Setting | Type | Default | Description |
| ------- | ---- | ------- | ----------- |
| `recv_udp_payload_size` | Integer | `1350` | Maximum received UDP payload size |
| `send_udp_payload_size` | Integer | `1350` | Maximum sent UDP payload size |
| `initial_max_data` | Integer | `104857600` | Initial max connection data (100 MB) |
| `initial_max_stream_data_bidi_local` | Integer | `1048576` | Local bidirectional stream flow control (1 MB) |
| `initial_max_stream_data_bidi_remote` | Integer | `1048576` | Remote bidirectional stream flow control (1 MB) |
| `initial_max_stream_data_uni` | Integer | `1048576` | Unidirectional stream flow control (1 MB) |
| `initial_max_streams_bidi` | Integer | `4096` | Maximum bidirectional streams |
| `initial_max_streams_uni` | Integer | `4096` | Maximum unidirectional streams |
| `max_connection_window` | Integer | `25165824` | Maximum connection window (24 MB) |
| `max_stream_window` | Integer | `16777216` | Maximum stream window (16 MB) |
| `disable_active_migration` | Boolean | `true` | Disable active connection migration |
| `enable_early_data` | Boolean | `true` | Enable 0-RTT early data |
| `message_queue_capacity` | Integer | `4096` | QUIC multiplexer queue capacity |

### Forward Protocol Settings

Configure how the endpoint forwards connections.

#### Direct Forwarding (default)

```toml
[forward_protocol]
direct = {}
```

Routes connections directly to target hosts.

#### SOCKS5 Forwarding

```toml
[forward_protocol.socks5]
address = "127.0.0.1:1080"
extended_auth = false
```

| Setting | Type | Default | Description |
| ------- | ---- | ------- | ----------- |
| `address` | String | - | **Required.** SOCKS5 proxy address |
| `extended_auth` | Boolean | `false` | Enable extended authentication |

### Reverse Proxy Settings

Optional. Enables TLS termination and HTTP protocol translation.

```toml
[reverse_proxy]
server_address = "127.0.0.1:8080"
path_mask = "/api"
h3_backward_compatibility = false
```

| Setting | Type | Default | Description |
| ------- | ---- | ------- | ----------- |
| `server_address` | String | - | **Required.** Origin server address |
| `path_mask` | String | - | **Required.** Path prefix for routing (must start with `/`) |
| `h3_backward_compatibility` | Boolean | `false` | Override HTTP method for H3→H1 translation |

The reverse proxy translates HTTP/x traffic to HTTP/1.1 towards the origin
server. Translated requests include the `X-Original-Protocol` header (`HTTP1`,
`HTTP2`, or `HTTP3`).

### ICMP Settings

Optional. Enables ICMP forwarding. Requires superuser privileges on some systems.

```toml
[icmp]
interface_name = "eth0"
request_timeout_secs = 3
recv_message_queue_capacity = 256
```

| Setting | Type | Default | Description |
| ------- | ---- | ------- | ----------- |
| `interface_name` | String | `eth0` (Linux) / `en0` (macOS) | Network interface for ICMP socket |
| `request_timeout_secs` | Integer | `3` | ICMP request timeout in seconds |
| `recv_message_queue_capacity` | Integer | `256` | Message queue capacity per client |

### Metrics Settings

Optional. Enables Prometheus-compatible metrics endpoint.

```toml
[metrics]
address = "127.0.0.1:1987"
request_timeout_secs = 3
```

| Setting | Type | Default | Description |
| ------- | ---- | ------- | ----------- |
| `address` | String | `127.0.0.1:1987` | Metrics endpoint address |
| `request_timeout_secs` | Integer | `3` | Request timeout in seconds |

Metrics are served by a pull-only, unauthenticated HTTP listener. Keep the
default loopback address and let a trusted local process scrape it. The endpoint
does not push metrics or logs to any service.

---

## TLS Hosts Reference

Each TLS host entry requires:

| Field | Type | Description |
| ----- | ---- | ----------- |
| `hostname` | String | **Required.** Hostname for TLS SNI matching (must be unique) |
| `cert_chain_path` | String | **Required.** Path to PEM certificate chain file |
| `private_key_path` | String | **Required.** Path to PEM private key file |
| `allowed_sni` | Array of strings | Alternative accepted SNI values (optional) |

### Host Types

- **`main_hosts`** - Primary hosts for VPN traffic tunneling and service requests
- **`ping_hosts`** - Respond with `200 OK` to HTTPS GET requests (health checks)
- **`speedtest_hosts`** - Handle speed test requests:
    - `GET /Nmb.bin` (N=1-100): Download N megabytes
    - `POST /upload.html`: Upload test (up to 120 MB)
- **`reverse_proxy_hosts`** - Forward to reverse proxy server (requires `[reverse_proxy]`)

---

## Rules Reference

Rules filter incoming connections based on client IP and/or TLS client random data.

### Rule Structure

```toml
[[rule]]
cidr = "192.168.0.0/16"           # Optional: IP range in CIDR notation
client_random_prefix = "aabbcc"   # Optional: Hex-encoded prefix or prefix/mask
action = "allow"                  # Required: "allow" or "deny"
```

### Evaluation

1. Rules are evaluated in order
2. First matching rule's action is applied
3. If no rules match, connection is **allowed** by default
4. If both `cidr` and `client_random_prefix` are specified, both must match

### Client Random Matching

Two formats are supported:

**Simple prefix matching:**

```toml
client_random_prefix = "aabbcc"
```

Matches if TLS client random starts with `0xaabbcc`.

**Bitwise matching with mask:**

```toml
client_random_prefix = "a0b0/f0f0"
```

Matches if `(client_random & 0xf0f0) == (0xa0b0 & 0xf0f0)`.

### Examples

```toml
# Block specific IP range
[[rule]]
cidr = "192.168.1.0/24"
action = "deny"

# Allow specific client random prefix
[[rule]]
client_random_prefix = "deadbeef"
action = "allow"

# Block internal networks with specific client signature
[[rule]]
cidr = "10.0.0.0/8"
client_random_prefix = "bad0/ff00"
action = "deny"

# Catch-all deny (place last)
[[rule]]
action = "deny"
```

---

## Runtime Configuration

### Hot Reloading TLS Hosts

Replace the updated TOML and PEM files atomically, then send `SIGHUP` to the
endpoint process to reload TLS hosts settings without restarting:

```bash
kill -HUP $(pidof trusttunnel_endpoint)
```

This reloads the TLS hosts settings file specified at startup and the
certificate/key files it references. It does not reload `vpn.toml`, credentials,
rules, CLI arguments, or systemd settings; restart for those changes.

### Systemd Service

A systemd service template is provided. Default configuration assumes files in `/opt/trusttunnel/`:

```bash
# Install service
sudo install -m 0644 /opt/trusttunnel/trusttunnel.service.template \
  /etc/systemd/system/trusttunnel.service
sudo systemctl daemon-reload
sudo systemctl enable --now trusttunnel

# Reload TLS settings in the main process
sudo systemctl kill --kill-who=main --signal=HUP trusttunnel

# View logs
sudo journalctl -u trusttunnel -f
```

The default log destination is local stdout, which enters the systemd journal.
There is no remote error reporter or log uploader. Use `info` during routine
operation and restrict access to logs because diagnostic metadata can be
sensitive.

---

## See Also

- [README.md](README.md) - Quick start guide
- [DEVELOPMENT.md](DEVELOPMENT.md) - Development documentation
- [PROTOCOL.md](PROTOCOL.md) - Protocol specification
- [CHANGELOG.md](CHANGELOG.md) - Changelog
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) - Runtime architecture
- [docs/PRIVACY.md](docs/PRIVACY.md) - Privacy and network boundaries
- [docs/SOURCE_BUILDS.md](docs/SOURCE_BUILDS.md) - Build and low-memory VPS deployment
