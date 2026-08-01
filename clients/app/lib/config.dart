import 'package:toml/toml.dart';

class VpnConfig {
  static const String placeholderHost = 'vpn.example.invalid';
  static const String placeholderAddress = '192.0.2.1:443';
  static const String placeholderCredential = 'replace-me';

  static const Set<String> _flatEndpointFields = {
    'hostname',
    'addresses',
    'custom_sni',
    'has_ipv6',
    'username',
    'password',
    'client_random_prefix',
    'skip_verification',
    'certificate',
    'upstream_protocol',
    'anti_dpi',
    'name',
    'dns_upstreams',
  };

  static const String defaultConfig =
      '''
loglevel = "info"
vpn_mode = "general"
killswitch_enabled = true
post_quantum_group_enabled = true
exclusions = []

[endpoint]
name = "My TrustTunnel server"
hostname = "$placeholderHost"
addresses = ["$placeholderAddress"]
username = "$placeholderCredential"
password = "$placeholderCredential"
client_random = ""
skip_verification = false
upstream_protocol = "http2"
anti_dpi = false
dns_upstreams = []

[listener]

[listener.tun]
bound_if = ""
included_routes = ["0.0.0.0/0", "2000::/3"]
excluded_routes = ["0.0.0.0/8", "10.0.0.0/8", "169.254.0.0/16", "172.16.0.0/12", "192.168.0.0/16", "224.0.0.0/3"]
mtu_size = 1500
''';

  static String normalize(String source) {
    late final Map<String, dynamic> document;
    try {
      document = TomlDocument.parse(source).toMap();
    } on Object {
      return source;
    }

    if (document['endpoint'] is Map || !_isFlatEndpointExport(document)) {
      return source;
    }

    final importedName = document['name'];
    final normalized = <String, dynamic>{
      'loglevel': 'info',
      'vpn_mode': 'general',
      'killswitch_enabled': true,
      'post_quantum_group_enabled': true,
      'exclusions': <String>[],
      'endpoint': <String, dynamic>{
        'name': importedName is String && importedName.trim().isNotEmpty
            ? importedName
            : 'My TrustTunnel server',
        'hostname': document['hostname'],
        'addresses': document['addresses'],
        'custom_sni': document['custom_sni'] ?? '',
        'has_ipv6': document['has_ipv6'] ?? true,
        'username': document['username'],
        'password': document['password'],
        'client_random': document['client_random_prefix'] ?? '',
        'skip_verification': document['skip_verification'] ?? false,
        'certificate': document['certificate'] ?? '',
        'upstream_protocol': document['upstream_protocol'],
        'anti_dpi': document['anti_dpi'] ?? false,
        'dns_upstreams': document['dns_upstreams'] ?? <String>[],
      },
      'listener': <String, dynamic>{
        'tun': <String, dynamic>{
          'bound_if': '',
          'included_routes': <String>['0.0.0.0/0', '2000::/3'],
          'excluded_routes': <String>[
            '0.0.0.0/8',
            '10.0.0.0/8',
            '169.254.0.0/16',
            '172.16.0.0/12',
            '192.168.0.0/16',
            '224.0.0.0/3',
          ],
          'mtu_size': 1500,
        },
      },
    };

    return TomlDocument.fromMap(normalized).toString();
  }

  static bool _isFlatEndpointExport(Map<String, dynamic> document) {
    if (!document.keys.every(_flatEndpointFields.contains)) {
      return false;
    }
    return document.containsKey('hostname') &&
        document.containsKey('addresses') &&
        document.containsKey('username') &&
        document.containsKey('password') &&
        document.containsKey('upstream_protocol');
  }

  static String? validate(String source) {
    late final Map<String, dynamic> document;
    try {
      document = TomlDocument.parse(source).toMap();
    } on Object catch (error) {
      return 'The configuration is not valid TOML: $error';
    }

    final endpointValue = document['endpoint'];
    if (endpointValue is! Map) {
      return 'The configuration needs an [endpoint] section.';
    }

    final hostname = endpointValue['hostname'];
    if (hostname is! String ||
        hostname.trim().isEmpty ||
        hostname == placeholderHost) {
      return 'Replace the endpoint hostname with the value exported by your server.';
    }

    final addresses = endpointValue['addresses'];
    if (addresses is! List ||
        addresses.isEmpty ||
        addresses.any(
          (address) =>
              address is! String ||
              address.trim().isEmpty ||
              address == placeholderAddress,
        )) {
      return 'Add at least one real endpoint address exported by your server.';
    }

    for (final field in ['username', 'password']) {
      final value = endpointValue[field];
      if (value is! String || value.isEmpty || value == placeholderCredential) {
        return 'Replace the endpoint $field with the value exported by your server.';
      }
    }

    final listener = document['listener'];
    if (listener is! Map || listener['tun'] is! Map) {
      return 'The configuration needs a [listener.tun] section.';
    }

    return null;
  }
}
