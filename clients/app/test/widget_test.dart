import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:trusttunnel_client_app/config.dart';
import 'package:trusttunnel_client_app/flutter_callbacks_impl.dart';
import 'package:trusttunnel_client_app/main.dart';
import 'package:trusttunnel_client_app/native_communication.dart';
import 'package:provider/provider.dart';

import 'test_config.dart';

void main() {
  test('deletes exported log snapshots and their empty directory', () async {
    final directory = await Directory.systemTemp.createTemp(
      'trusttunnel-log-cleanup-test-',
    );
    final first = File('${directory.path}/app.log')..writeAsStringSync('app');
    final second = File('${directory.path}/extension.log')
      ..writeAsStringSync('extension');

    expect(await deleteExportedLogSnapshots([first.path, second.path]), isTrue);
    expect(await directory.exists(), isFalse);
  });

  testWidgets('shows source-built client controls', (
    WidgetTester tester,
  ) async {
    await tester.pumpWidget(
      ChangeNotifierProvider(
        create: (_) => VpnStateNotifier(),
        child: const TrustTunnelApp(),
      ),
    );

    expect(find.text('TrustTunnel Client'), findsOneWidget);
    expect(find.text('Connect'), findsOneWidget);
    expect(find.text('View Local Logs'), findsOneWidget);
    expect(find.text('State: disconnected'), findsOneWidget);
  });

  testWidgets('native callbacks drive connection controls', (
    WidgetTester tester,
  ) async {
    final notifier = VpnStateNotifier();
    await tester.pumpWidget(
      ChangeNotifierProvider.value(
        value: notifier,
        child: const TrustTunnelApp(),
      ),
    );

    expect(find.text('Connect'), findsOneWidget);
    expect(
      tester
          .widget<OutlinedButton>(
            find.widgetWithText(OutlinedButton, 'Reconnect'),
          )
          .onPressed,
      isNull,
    );

    notifier.onStateChanged(VpnState.connecting);
    await tester.pump();

    expect(find.text('State: connecting'), findsOneWidget);
    expect(find.text('Disconnect'), findsOneWidget);
    expect(
      tester
          .widget<OutlinedButton>(
            find.widgetWithText(OutlinedButton, 'Reconnect'),
          )
          .onPressed,
      isNotNull,
    );

    notifier.onStateChanged(VpnState.disconnected);
    await tester.pump();

    expect(find.text('Connect'), findsOneWidget);
  });

  testWidgets('disconnect waits for native state and ignores repeat taps', (
    WidgetTester tester,
  ) async {
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final stopChannel = BasicMessageChannel<Object?>(
      'dev.flutter.pigeon.org_trusttunnel_client.NativeVpnInterface.stop',
      NativeVpnInterface.pigeonChannelCodec,
    );
    var stopCalls = 0;
    messenger.setMockDecodedMessageHandler<Object?>(stopChannel, (_) async {
      stopCalls++;
      return <Object?>[null];
    });
    addTearDown(
      () => messenger.setMockDecodedMessageHandler<Object?>(stopChannel, null),
    );

    final notifier = VpnStateNotifier()..onStateChanged(VpnState.connected);
    await tester.pumpWidget(
      ChangeNotifierProvider.value(
        value: notifier,
        child: const TrustTunnelApp(),
      ),
    );

    await tester.tap(find.widgetWithText(FilledButton, 'Disconnect'));
    await tester.tap(find.widgetWithText(FilledButton, 'Disconnect'));
    await tester.pumpAndSettle();

    expect(stopCalls, 1);
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Disconnect'))
          .onPressed,
      isNull,
    );

    notifier.onStateChanged(VpnState.disconnected);
    await tester.pump();

    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Connect'))
          .onPressed,
      isNotNull,
    );
  });

  testWidgets('immediate stop exception releases pending disconnect', (
    WidgetTester tester,
  ) async {
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final stopChannel = BasicMessageChannel<Object?>(
      'dev.flutter.pigeon.org_trusttunnel_client.NativeVpnInterface.stop',
      NativeVpnInterface.pigeonChannelCodec,
    );
    messenger.setMockDecodedMessageHandler<Object?>(
      stopChannel,
      (_) async => <Object?>['stop-failed', 'rejected', null],
    );
    addTearDown(
      () => messenger.setMockDecodedMessageHandler<Object?>(stopChannel, null),
    );

    final notifier = VpnStateNotifier()..onStateChanged(VpnState.connected);
    await tester.pumpWidget(
      ChangeNotifierProvider.value(
        value: notifier,
        child: const TrustTunnelApp(),
      ),
    );

    await tester.tap(find.widgetWithText(FilledButton, 'Disconnect'));
    await tester.pumpAndSettle();

    expect(find.textContaining('VPN operation failed:'), findsOneWidget);
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Disconnect'))
          .onPressed,
      isNotNull,
    );
  });

  testWidgets('start waits for native state and ignores rapid double taps', (
    WidgetTester tester,
  ) async {
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final startChannel = BasicMessageChannel<Object?>(
      'dev.flutter.pigeon.org_trusttunnel_client.NativeVpnInterface.start',
      NativeVpnInterface.pigeonChannelCodec,
    );
    var startCalls = 0;
    String? startedConfig;
    messenger.setMockDecodedMessageHandler<Object?>(startChannel, (
      Object? message,
    ) async {
      startCalls++;
      startedConfig = (message! as List<Object?>).single! as String;
      return <Object?>[null];
    });
    addTearDown(
      () => messenger.setMockDecodedMessageHandler<Object?>(startChannel, null),
    );

    final notifier = VpnStateNotifier();
    await tester.pumpWidget(
      ChangeNotifierProvider.value(
        value: notifier,
        child: const TrustTunnelApp(),
      ),
    );
    await tester.enterText(
      find.byType(EditableText).last,
      serverEndpointExport,
    );

    await tester.tap(find.widgetWithText(FilledButton, 'Connect'));
    await tester.tap(find.widgetWithText(FilledButton, 'Connect'));
    await tester.pumpAndSettle();

    expect(startCalls, 1);
    expect(startedConfig, isNotNull);
    expect(startedConfig, isNot(serverEndpointExport));
    expect(VpnConfig.validate(startedConfig!), isNull);
    expect(startedConfig, contains('[endpoint]'));
    expect(startedConfig, contains('[listener.tun]'));
    expect(
      tester
          .widget<EditableText>(find.byType(EditableText).last)
          .controller
          .text,
      startedConfig,
    );
    expect(find.text('State: disconnected'), findsOneWidget);
    expect(find.text('Connect'), findsOneWidget);
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Connect'))
          .onPressed,
      isNull,
    );

    notifier.onStateChanged(VpnState.connecting);
    await tester.pump();
    expect(find.text('Disconnect'), findsOneWidget);
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Disconnect'))
          .onPressed,
      isNotNull,
    );
  });

  testWidgets('start failure callback releases pending connection', (
    WidgetTester tester,
  ) async {
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final startChannel = BasicMessageChannel<Object?>(
      'dev.flutter.pigeon.org_trusttunnel_client.NativeVpnInterface.start',
      NativeVpnInterface.pigeonChannelCodec,
    );
    messenger.setMockDecodedMessageHandler<Object?>(
      startChannel,
      (_) async => <Object?>[null],
    );
    addTearDown(
      () => messenger.setMockDecodedMessageHandler<Object?>(startChannel, null),
    );

    final notifier = VpnStateNotifier();
    await tester.pumpWidget(
      ChangeNotifierProvider.value(
        value: notifier,
        child: const TrustTunnelApp(),
      ),
    );
    await tester.enterText(
      find.byType(EditableText).last,
      serverEndpointExport,
    );

    await tester.tap(find.widgetWithText(FilledButton, 'Connect'));
    await tester.pumpAndSettle();

    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Connect'))
          .onPressed,
      isNull,
    );

    notifier.onStateChanged(VpnState.disconnected);
    await tester.pump();

    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Connect'))
          .onPressed,
      isNotNull,
    );
  });

  testWidgets('immediate start exception releases pending connection', (
    WidgetTester tester,
  ) async {
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final startChannel = BasicMessageChannel<Object?>(
      'dev.flutter.pigeon.org_trusttunnel_client.NativeVpnInterface.start',
      NativeVpnInterface.pigeonChannelCodec,
    );
    messenger.setMockDecodedMessageHandler<Object?>(
      startChannel,
      (_) async => <Object?>['start-failed', 'rejected', null],
    );
    addTearDown(
      () => messenger.setMockDecodedMessageHandler<Object?>(startChannel, null),
    );

    await tester.pumpWidget(
      ChangeNotifierProvider(
        create: (_) => VpnStateNotifier(),
        child: const TrustTunnelApp(),
      ),
    );
    await tester.enterText(
      find.byType(EditableText).last,
      serverEndpointExport,
    );

    await tester.tap(find.widgetWithText(FilledButton, 'Connect'));
    await tester.pumpAndSettle();

    expect(find.textContaining('VPN operation failed:'), findsOneWidget);
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Connect'))
          .onPressed,
      isNotNull,
    );
  });

  testWidgets('reconnect waits for stop state then starts normalized config', (
    WidgetTester tester,
  ) async {
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final startChannel = BasicMessageChannel<Object?>(
      'dev.flutter.pigeon.org_trusttunnel_client.NativeVpnInterface.start',
      NativeVpnInterface.pigeonChannelCodec,
    );
    final stopChannel = BasicMessageChannel<Object?>(
      'dev.flutter.pigeon.org_trusttunnel_client.NativeVpnInterface.stop',
      NativeVpnInterface.pigeonChannelCodec,
    );
    String? startedConfig;
    var stopCalls = 0;
    messenger.setMockDecodedMessageHandler<Object?>(startChannel, (
      Object? message,
    ) async {
      startedConfig = (message! as List<Object?>).single! as String;
      return <Object?>[null];
    });
    messenger.setMockDecodedMessageHandler<Object?>(stopChannel, (_) async {
      stopCalls++;
      return <Object?>[null];
    });
    addTearDown(() {
      messenger.setMockDecodedMessageHandler<Object?>(startChannel, null);
      messenger.setMockDecodedMessageHandler<Object?>(stopChannel, null);
    });

    final notifier = VpnStateNotifier()..onStateChanged(VpnState.connected);
    await tester.pumpWidget(
      ChangeNotifierProvider.value(
        value: notifier,
        child: const TrustTunnelApp(),
      ),
    );
    await tester.enterText(
      find.byType(EditableText).last,
      serverEndpointExport,
    );

    await tester.tap(find.widgetWithText(OutlinedButton, 'Reconnect'));
    await tester.pumpAndSettle();

    expect(stopCalls, 1);
    expect(startedConfig, isNull);
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Disconnect'))
          .onPressed,
      isNull,
    );
    expect(
      tester
          .widget<OutlinedButton>(
            find.widgetWithText(OutlinedButton, 'Reconnect'),
          )
          .onPressed,
      isNull,
    );

    notifier.onStateChanged(VpnState.disconnected);
    await tester.pumpAndSettle();

    expect(startedConfig, isNotNull);
    expect(VpnConfig.validate(startedConfig!), isNull);
    expect(startedConfig, contains('[endpoint]'));
    expect(startedConfig, contains('[listener.tun]'));
    expect(
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Connect'))
          .onPressed,
      isNull,
    );

    notifier.onStateChanged(VpnState.connecting);
    await tester.pump();

    expect(
      tester
          .widget<OutlinedButton>(
            find.widgetWithText(OutlinedButton, 'Reconnect'),
          )
          .onPressed,
      isNotNull,
    );
  });
}
