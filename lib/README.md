# TrustTunnel endpoint

## Building the library

### Prerequisites

- Rust 1.95, selected by the repository-root `rust-toolchain.toml`
- libclang 9.0 or higher

### Building

Execute the following commands in the Terminal:

```shell
cargo build
```

to build the debug version, or

```shell
cargo build --release
```

to build the release version.

## Features description

### Traffic forwarding

As for now, the endpoint can demultiplex client's connections multiplexed in either HTTP/1, or
HTTP/2, or HTTP/3 session. An application can set up how the endpoint forwards the demultiplexed
client's connection by setting `Settings.forward_protocol`. The available options
(see `settings.ForwardProtocolSettings`) are:

- routing a connection directly to its target host
- routing a connection though a SOCKS5 proxy

#### ICMP forwarding

As an optional feature, the endpoint can also forward ICMP packets from a client. This feature
can be set up by setting `Settings.icmp`. An application MUST set up an interface name to bind
the ICMP socket to, and MAY tweak some other settings, like the timeouts and message queue size.

### Reverse proxy

Client's connection is treated as a reverse proxy stream in the following cases:

1) A TLS session or QUIC connection has the SNI set to the host name equal to one
   from `TlsHostsSettings.reverse_proxy`.
2) If a request path starts with `ReverseProxySettings.path_mask`, it is routed to reverse proxy.
3) Otherwise, routing is defined by `ping_path` and `speedtest_path` configuration.
   Requests that do not match ping, speedtest, or reverse proxy rules are treated as tunnel requests.

The stream is used for mutual client and endpoint notifications and some control messages.
The endpoint does TLS termination on such connections and translates HTTP/x traffic into
HTTP/1.1 protocol towards the server and back into original HTTP/x towards the client.
Like this:

```(client) TLS(HTTP/x) <--(endpoint)--> (server) HTTP/1.1```

The translated HTTP/1.1 requests have the custom header `X-Original-Protocol` appended.
For now, its value can be `HTTP1`, `HTTP2`, or `HTTP3`.

Note: HTTP/3 reverse proxy handling keeps the write side open when the client finishes sending
the request body, to avoid truncating large responses.

### Authentication

#### Client authentication options

##### SNI authentication

A compatible client can connect with SNI set to
`encoded_credentials.domain_name`, where:

- `encoded_credentials` is the Base64 encoding of `username:password`;
- `domain_name` is the endpoint's configured main hostname.

The endpoint treats the first label as authentication data and still selects
the certificate and tunnel settings for `domain_name`.

##### Proxy authentication

A client connects to the endpoint using the proxy HTTP authentication mechanism with
the "basic" scheme:

```text
Proxy-Authorization: Basic base64(username + ':' + password)
```

#### Endpoint authentication methods

The endpoint binary reads `credentials_file` from `vpn.toml`. That file is TOML
with one `[[client]]` table per credential:

```toml
[[client]]
username = "alice"
password = "use-a-long-random-secret"

[[client]]
username = "bob"
password = "use-a-different-long-random-secret"
```

It creates an
`authentication::registry_based::RegistryBasedAuthenticator` from these
entries. This authenticator compares the encoded Basic credential supplied by
proxy or SNI authentication against the configured registry. The optional
`max_http2_conns` and `max_http3_conns` keys apply per-credential connection
limits; see the [configuration reference](../CONFIGURATION.md#credentials-file-credentialstoml).

Library users can instead implement the `authentication::Authenticator` trait
and pass their implementation to `core::Core::new`. Passing no authenticator
leaves tunnel requests unauthenticated, so the endpoint binary rejects that
configuration on a public listen address.

##### SOCKS5 authenticator

###### Standard authentication

In case `Socks5ForwarderSettings.extended_auth` is set to false, the endpoint performs
the standard authentication procedure according to the
[RFC 1929](https://datatracker.ietf.org/doc/html/rfc1929).

Depending on the client-side authentication way, the username and password are as follows:

- [SNI authentication](#sni-authentication):
    - both the SOCKS5 `username` and `password` are the encoded credential from
      the SNI prefix

- [Proxy authentication](#proxy-authentication):
    - the Basic value is decoded and its `username` and `password` are forwarded
      as SOCKS5 username/password authentication

###### Extended authentication

The extended authentication uses `0x80` as an authentication method.
After a server selects this authentication method, a client sends an authentication
request in the following format:

```text
+-----+-----------+-----+--------+
| VER |   EXT(0)  |     | EXT(n) |
+-----+-----------+ ... +--------+
|  1  | see below |     |        |
+-----+-----------+-----+--------+
```

Where:

- `VER` - the current extended authentication version: 0x01
- `EXT[i]` - an extension in the following format:

   ```text
   +------+--------+----------+
   | TYPE | LENGTH |   VALUE  |
   +------+--------+----------+
   |  1   |    2   | Variable |
   +------+--------+----------+
   ```

   Where:
    - `TYPE` - a type of the extension value (see [`ExtendedAuthenticationValue`])
    - `LENGTH` - the length of the extension value
    - `VALUE` - the extension value

Available extensions:

- `TERM`: type = 0x00, length = 0 - terminating extension, marks a message end
- `DOMAIN`: type = 0x01, length = (0..MAX], value = UTF-8 string - hostname which
  a client used for the TLS session (SNI)
- `CLIENT_ADDRESS`: type = 0x02, length = [4|16], value = Bytes - public IP
  address of the VPN client
- `USER_AGENT`: type = 0x03, length = (0..MAX], value = UTF-8 string - user agent of the VPN client
- `PROXY_AUTH`: type = 0x04, length = (0..MAX], value = base64 string - `<credentials>` part of
  [the Proxy-Authorization header](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Proxy-Authorization)
- `SNI_AUTH`: type = 0x05, length = 0 - marks that the VPN client tries to authenticate using SNI

A message **MUST** end with the `TERM` extension.

The server responds with a standard message as in [the RFC](https://datatracker.ietf.org/doc/html/rfc1929#section-2).

### Metrics collecting

In order to collect some metrics of a running endpoint, an application can set up it to listen for
the metrics collecting requests (see `Settings.metrics`). An endpoint running with this feature
will listen on the configured address (`MetricsSettings.address`) for plain HTTP/1 requests.
The following paths are available:

- `/health-check` - used for pinging the endpoint, so it will respond with `200 OK`
- `/metrics` - used for metrics collecting, so it will respond with a bunch of values according to
  [the prometheus specification](https://prometheus.io/)

## License

Apache 2.0
