# TrustTunnel Architecture

This document explains the endpoint from the viewpoint of an operator who is
new to VPN internals. For message formats and interoperability requirements,
use the [protocol specification](../PROTOCOL.md).

## Components

The monorepo has two runtime sides:

```text
client device                                      Linux VPS

clients/app                                        endpoint/
Flutter interface                                  small CLI wrapper
     |                                                  |
     v                                                  v
clients/engine                                     lib/
TUN, routes, DNS, protocol                         TLS, HTTP, auth, forwarding
     |                                                  |
     +---------------- encrypted tunnel ---------------+
```

The remaining server crates support these runtimes:

- `tools/` builds `setup_wizard`, which creates endpoint configuration and can
  provision a certificate.
- `deeplink/` encodes and decodes the `tt://` client-configuration format.
- `macros/` contains compile-time helpers shared by the Rust crates.

The setup wizard is not a daemon. It runs only when the operator invokes it.

## Packet path

For a system-wide graphical client, the ordinary path is:

```text
1. An application asks the operating system to reach a destination.
2. The operating system routes that packet into the client's TUN interface.
3. The client engine reconstructs the TCP, UDP, DNS, or ICMP operation.
4. The client represents that operation inside TrustTunnel HTTP messages.
5. TLS protects those messages on their way to the endpoint.
6. The endpoint authenticates the client and validates the destination.
7. The endpoint opens an ordinary socket to the requested destination.
8. Replies follow the same path in reverse.
```

A command-line client may use a local SOCKS5 listener instead of a TUN. The
first and second steps then become an application connecting to that local
proxy. The protocol between client engine and endpoint is otherwise the same.

### TCP

```text
application TCP stream
        |
client TCP/IP stack or SOCKS listener
        |
TrustTunnel stream inside HTTP/TLS
        |
endpoint direct forwarder
        |
new outbound TCP socket from the VPS
        |
target TCP service
```

The target sees the VPS public address as the source. The endpoint relays bytes
in both directions and enforces its connection and idle timeouts.

### UDP

UDP has no connection handshake, but both sides keep short-lived per-flow state
so reply datagrams return to the correct local application. The endpoint sends
ordinary UDP datagrams from the VPS. UDP tunnel state expires according to
`udp_connections_timeout_secs`.

### ICMP

ICMP forwarding is optional. It is commonly used by ping and path diagnostics.
Unlike ordinary TCP and UDP sockets, it may require `CAP_NET_RAW` or root on
Linux. Leave `[icmp]` absent unless it is needed. The client and endpoint can
still carry normal web and DNS traffic without ICMP forwarding.

### DNS

DNS is application traffic, not a control connection to the TrustTunnel
project. The client intercepts DNS according to its local configuration. A DNS
query sent through the tunnel ultimately reaches the operator-selected resolver
from the VPS like other forwarded traffic. If a requested destination is a
hostname rather than an IP address, the endpoint may also use the VPS operating
system resolver to create its outbound connection.

Before changing device routes or DNS, the client captures the device's current
resolver and resolves configured endpoint hostnames. It also uses that captured
resolver to pre-resolve hostname-based encrypted DNS upstreams before those
connections enter the tunnel. With no custom upstream, it leaves the device's
DNS setting unchanged. There is no built-in public fallback; a path requiring a
missing system resolver fails closed.

Do not assume DNS is encrypted merely because the client-to-endpoint tunnel is
encrypted. A plain DNS upstream is plaintext between the VPS and that resolver.
Choose a resolver and transport that match the deployment's privacy needs.

## Transport layers

TrustTunnel supports three HTTP transports:

| Transport | Underlying network | Typical listener |
| --- | --- | --- |
| HTTP/1.1 | TLS over TCP | TCP 443 |
| HTTP/2 | TLS over TCP | TCP 443 |
| HTTP/3 | QUIC, including TLS, over UDP | UDP 443 |

HTTP/2 and HTTP/3 can multiplex several logical streams over fewer network
connections. HTTP/1.1 uses a less capable HTTP transport but remains useful as
a compatibility baseline.

The endpoint can bind TCP and UDP to the same numeric address and port because
they are different transport protocols:

```text
vpn.example.com:443/tcp -> TLS -> HTTP/1.1 or HTTP/2
vpn.example.com:443/udp -> QUIC/TLS -> HTTP/3
```

Disabling the QUIC section stops the UDP listener. Disabling both HTTP/1.1 and
HTTP/2 stops useful TCP tunnel service. At least one listener protocol must be
configured.

## TLS, SNI, and ALPN

These three terms describe different parts of the same connection setup:

- **TLS** encrypts and authenticates the connection between client and VPS.
  The client must trust the certificate presented by the endpoint.
- **SNI**, or Server Name Indication, is the hostname offered by the client in
  the TLS handshake. The endpoint uses it to select a configured host and
  certificate. SNI and other connection metadata may be observable to the
  network depending on the TLS deployment.
- **ALPN**, or Application-Layer Protocol Negotiation, lets the peers select
  `http/1.1`, `h2`, or `h3` during connection establishment.

```text
client hello
|-- SNI: vpn.example.com       selects hosts.toml entry and certificate
`-- ALPN: h2, http/1.1         selects an enabled HTTP codec
```

The exported client address, the SNI used by the client, a name on the TLS
certificate, and a `main_hosts` entry normally all refer to the same hostname.
`allowed_sni` and the `--custom-sni` export option support deliberate advanced
layouts, but they should not be used to hide an accidental certificate or DNS
mismatch.

## Encryption boundary

There can be two independent encrypted connections for an HTTPS application:

```text
application HTTPS:

application ===== target TLS, relayed as bytes ==================> website
             \                                                   /
              \ TrustTunnel TLS                                 /
               client ================================> endpoint

TrustTunnel TLS ends here -----------------------------^
target TLS continues until the website ------------------------------^
```

For plain HTTP or another plaintext protocol, only the client-to-endpoint leg
has TrustTunnel TLS. The endpoint necessarily sees the bytes it must forward,
and the VPS-to-target leg remains plaintext. Trust the VPS administrator and
prefer end-to-end encrypted application protocols.

## Why the VPS needs no TUN or NAT

A routed VPN normally accepts IP packets into a server interface and asks the
kernel to route and masquerade them. TrustTunnel's default endpoint instead
turns each accepted request into a userspace socket:

```text
routed VPN                                 TrustTunnel direct forwarder

server TUN                                endpoint process
   |                                          |
kernel forwarding                             +-- connect(TCP target)
   |                                          +-- sendto(UDP target)
NAT/MASQUERADE                                `-- optional raw ICMP
   |                                          |
Internet                                  Internet
```

Therefore, a native endpoint does not need:

- `/dev/net/tun`;
- `net.ipv4.ip_forward=1` or `net.ipv6.conf.all.forwarding=1`;
- `iptables` or `nftables` masquerading rules;
- an inbound range of ephemeral ports.

It does need normal return traffic for its outbound sockets. Stateful host and
cloud firewalls normally permit those replies automatically. Docker adds its
own bridge and published-port networking, which is separate from TrustTunnel.

## Endpoint request handling

After TLS and HTTP selection, configured hostnames and paths select a channel:

- **tunnel** authenticates the client and forwards requested traffic;
- **ping** returns a local health response when enabled;
- **speed test** sends or consumes test data when enabled;
- **reverse proxy** translates supported requests to HTTP/1.1 and sends them to
  the configured origin.

The default direct forwarder opens sockets itself. An optional SOCKS5 forwarder
sends requests to the operator-configured SOCKS proxy instead. These optional
destinations are deployment choices and are not vendor services.

Rules in `rules.toml` filter the incoming connection by source network and/or a
TLS client-random prefix. They are admission controls, not destination firewall
rules. `allow_private_network_connections = false` separately prevents clients
from using the endpoint to reach addresses classified as private.

## Configuration lifecycle

The endpoint loads the following at process start:

```text
vpn.toml
|-- general settings
|-- path to credentials.toml
|-- optional path to rules.toml
`-- listener and forwarder configuration

hosts.toml
`-- TLS hostnames, certificate chains, and private-key paths
```

`SIGHUP` reloads `hosts.toml` and the referenced certificate/key material.
Changes to `vpn.toml`, credentials, or rules require a restart. Use atomic file
replacement so a reload never observes a partially written PEM or TOML file.

The metrics listener is optional and pull-based. With its default
`127.0.0.1:1987` address, only a local monitoring process can request metrics.
The endpoint does not push metrics anywhere.

## Terminology

**Endpoint**
: The TrustTunnel server process on the VPS. It terminates tunnel TLS and opens
  onward connections.

**Client engine**
: Native code on the user's device that manages routes, DNS, packet processing,
  and the tunnel connection.

**TUN**
: A virtual operating-system interface carrying IP packets. It exists on the
  client in the normal graphical deployment.

**TLS certificate**
: A signed statement binding a public key to a hostname. The endpoint holds the
  private key; clients verify the certificate before sending credentials.

**SNI**
: The requested server hostname in a TLS handshake, used for virtual-host and
  certificate selection.

**ALPN**
: A TLS/QUIC negotiation field used here to select HTTP/1.1, HTTP/2, or HTTP/3.

**HTTP/1.1, HTTP/2, and HTTP/3**
: Successive HTTP transports. HTTP/3 uses QUIC; the first two use TCP.

**QUIC**
: A secure multiplexed transport over UDP and the foundation of HTTP/3.

**TCP**
: A reliable ordered byte stream used by HTTPS, SSH, and many other protocols.

**UDP**
: A datagram transport used by DNS, real-time media, QUIC, and other protocols.

**ICMP**
: An IP control and diagnostic protocol used by tools such as ping.

**DNS**
: The system that maps hostnames to IP addresses. The resolver is chosen by the
  operating system or client configuration, not by the TrustTunnel project.

**Forwarder**
: Endpoint logic that sends an authenticated client's requested connection
  directly or through a configured SOCKS5 proxy.

**Reverse proxy**
: Optional endpoint logic that serves configured HTTPS host/path traffic to a
  configured HTTP origin rather than treating it as a VPN tunnel request.

## Security properties and limits

TrustTunnel is designed to use widely deployed HTTPS transports. That design
does not promise traffic indistinguishability or censorship resistance against
every observer. An observer may use IP addresses, DNS, SNI, TLS and QUIC
fingerprints, packet sizes, timing, active probes, endpoint reputation, or
local device access. Configuration errors can also make a connection distinct.

TLS protects the tunnel in transit; it does not make the endpoint untrusted.
The endpoint can observe destination metadata and plaintext application data,
and a compromised client or VPS remains outside the protection of the
protocol. Review [the privacy boundary](PRIVACY.md) before deployment.
