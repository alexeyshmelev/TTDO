const String serverEndpointExport = '''
hostname = "vpn.operator.test"
addresses = ["203.0.113.10:443"]
custom_sni = "front.operator.test"
has_ipv6 = false
username = "alice"
password = "local-credential"
client_random_prefix = "a1b2/fff0"
skip_verification = false
certificate = ""
upstream_protocol = "http2"
anti_dpi = true
name = "Operator VPN"
dns_upstreams = ["tls://dns.operator.test"]
''';
