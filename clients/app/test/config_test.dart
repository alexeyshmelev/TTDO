import 'package:flutter_test/flutter_test.dart';
import 'package:toml/toml.dart';
import 'package:trusttunnel_client_app/config.dart';
import 'package:trusttunnel_client_app/flutter_callbacks_impl.dart';

import 'test_config.dart';

void main() {
  test('default template cannot be connected accidentally', () {
    expect(VpnConfig.validate(VpnConfig.defaultConfig), isNotNull);
  });

  test('server-exported values pass validation', () {
    final configured = VpnConfig.defaultConfig
        .replaceFirst(VpnConfig.placeholderHost, 'vpn.operator.test')
        .replaceFirst(VpnConfig.placeholderAddress, '203.0.113.10:443')
        .replaceAll(VpnConfig.placeholderCredential, 'local-credential');

    expect(VpnConfig.validate(configured), isNull);
  });

  test('flat server export normalizes into a complete native config', () {
    final normalized = VpnConfig.normalize(serverEndpointExport);
    final document = TomlDocument.parse(normalized).toMap();
    final endpoint = document['endpoint'] as Map<String, dynamic>;
    final listener = document['listener'] as Map<String, dynamic>;

    expect(VpnConfig.validate(normalized), isNull);
    expect(document['loglevel'], 'info');
    expect(document['vpn_mode'], 'general');
    expect(document['killswitch_enabled'], isTrue);
    expect(document['post_quantum_group_enabled'], isTrue);
    expect(document['exclusions'], isEmpty);
    expect(endpoint['name'], 'Operator VPN');
    expect(endpoint['hostname'], 'vpn.operator.test');
    expect(endpoint['addresses'], ['203.0.113.10:443']);
    expect(endpoint['custom_sni'], 'front.operator.test');
    expect(endpoint['has_ipv6'], isFalse);
    expect(endpoint['username'], 'alice');
    expect(endpoint['password'], 'local-credential');
    expect(endpoint['client_random'], 'a1b2/fff0');
    expect(endpoint.containsKey('client_random_prefix'), isFalse);
    expect(endpoint['skip_verification'], isFalse);
    expect(endpoint['certificate'], isEmpty);
    expect(endpoint['upstream_protocol'], 'http2');
    expect(endpoint['anti_dpi'], isTrue);
    expect(endpoint['dns_upstreams'], ['tls://dns.operator.test']);
    expect(listener['tun'], isA<Map>());
    expect(normalized, contains('[endpoint]'));
    expect(normalized, contains('[listener.tun]'));
    expect(VpnConfig.normalize(normalized), normalized);
  });

  test('complete client TOML remains byte-for-byte unchanged', () {
    final configured = VpnConfig.defaultConfig
        .replaceFirst(VpnConfig.placeholderHost, 'vpn.operator.test')
        .replaceFirst(VpnConfig.placeholderAddress, '203.0.113.10:443')
        .replaceAll(VpnConfig.placeholderCredential, 'local-credential');

    expect(VpnConfig.normalize(configured), configured);
  });

  test('flat server export without a display name gets the native default', () {
    final unnamed = serverEndpointExport.replaceFirst(
      'name = "Operator VPN"',
      'name = ""',
    );
    final normalized = TomlDocument.parse(VpnConfig.normalize(unnamed)).toMap();

    expect((normalized['endpoint'] as Map)['name'], 'My TrustTunnel server');
  });

  test('invalid TOML returns a useful validation error', () {
    expect(VpnConfig.validate('[endpoint'), contains('not valid TOML'));
  });

  test('native callback ignores out-of-range states', () {
    final notifier = VpnStateNotifier();
    final callbacks = FlutterCallbacksImpl(notifier);

    callbacks.onStateChanged(-1);
    callbacks.onStateChanged(VpnState.values.length);

    expect(notifier.state, VpnState.disconnected);
  });
}
