import 'dart:async' show Completer;
import 'dart:io' show Directory, File, FileSystemException;

import 'package:code_text_field/code_text_field.dart';
import 'package:flutter/material.dart';
import 'package:flutter_highlight/themes/gruvbox-dark.dart';
import 'package:path/path.dart' as path;
import 'package:provider/provider.dart';
import 'package:trusttunnel_client_app/flutter_callbacks_impl.dart';
import 'package:trusttunnel_client_app/native_communication.dart';

import 'config.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final notifier = VpnStateNotifier();
  FlutterCallbacks.setUp(FlutterCallbacksImpl(notifier));
  runApp(
    ChangeNotifierProvider.value(
      value: notifier,
      child: const TrustTunnelApp(),
    ),
  );
}

class TrustTunnelApp extends StatelessWidget {
  const TrustTunnelApp({super.key});

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'TrustTunnel Client',
    theme: ThemeData(
      colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff315bef)),
      useMaterial3: true,
    ),
    home: const ClientScreen(),
  );
}

class ClientScreen extends StatefulWidget {
  const ClientScreen({super.key});

  @override
  State<ClientScreen> createState() => _ClientScreenState();
}

class _ClientScreenState extends State<ClientScreen> {
  final CodeController _configuration = CodeController(
    text: VpnConfig.defaultConfig,
  );
  final NativeVpnInterface _nativeVpn = NativeVpnInterface();
  VpnStateNotifier? _vpnStateNotifier;
  Completer<void>? _stopStateCompleted;
  bool _operationInProgress = false;
  bool _waitingForStartState = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final notifier = context.read<VpnStateNotifier>();
    if (identical(notifier, _vpnStateNotifier)) {
      return;
    }
    _vpnStateNotifier?.removeListener(_handleVpnStateChanged);
    _vpnStateNotifier = notifier..addListener(_handleVpnStateChanged);
  }

  @override
  void dispose() {
    _vpnStateNotifier?.removeListener(_handleVpnStateChanged);
    final stopStateCompleted = _stopStateCompleted;
    if (stopStateCompleted != null && !stopStateCompleted.isCompleted) {
      stopStateCompleted.complete();
    }
    _configuration.dispose();
    super.dispose();
  }

  void _handleVpnStateChanged() {
    final stopStateCompleted = _stopStateCompleted;
    if (stopStateCompleted != null) {
      if (_vpnStateNotifier?.state == VpnState.disconnected &&
          !stopStateCompleted.isCompleted) {
        stopStateCompleted.complete();
      }
      return;
    }
    if (_operationInProgress && _waitingForStartState) {
      _finishOperation();
    }
  }

  void _finishOperation() {
    _stopStateCompleted = null;
    _waitingForStartState = false;
    if (!_operationInProgress) {
      return;
    }
    if (mounted) {
      setState(() => _operationInProgress = false);
    } else {
      _operationInProgress = false;
    }
  }

  String? _configurationForNative() {
    final normalized = VpnConfig.normalize(_configuration.text);
    final validationError = VpnConfig.validate(normalized);
    if (validationError != null) {
      _showMessage(validationError);
      return null;
    }
    if (normalized != _configuration.text) {
      _configuration.text = normalized;
    }
    return normalized;
  }

  Future<void> _toggleConnection() async {
    if (_operationInProgress) {
      return;
    }

    final connectionActive =
        context.read<VpnStateNotifier>().state != VpnState.disconnected;
    final configuration = connectionActive ? null : _configurationForNative();
    if (!connectionActive && configuration == null) {
      return;
    }

    setState(() {
      _operationInProgress = true;
      _waitingForStartState = !connectionActive;
    });
    final stopStateCompleted = connectionActive ? Completer<void>() : null;
    _stopStateCompleted = stopStateCompleted;
    try {
      if (connectionActive) {
        await _nativeVpn.stop();
        await stopStateCompleted!.future;
        _stopStateCompleted = null;
        _finishOperation();
      } else {
        await _nativeVpn.start(configuration!);
      }
    } on Object catch (error) {
      _finishOperation();
      _showMessage('VPN operation failed: $error');
    }
  }

  Future<void> _reconnect() async {
    if (_operationInProgress ||
        context.read<VpnStateNotifier>().state == VpnState.disconnected) {
      return;
    }
    final configuration = _configurationForNative();
    if (configuration == null) {
      return;
    }

    setState(() {
      _operationInProgress = true;
      _waitingForStartState = false;
    });
    final stopStateCompleted = Completer<void>();
    _stopStateCompleted = stopStateCompleted;
    try {
      await _nativeVpn.stop();
      await stopStateCompleted.future;
      _stopStateCompleted = null;
      if (!mounted) {
        return;
      }
      _waitingForStartState = true;
      await _nativeVpn.start(configuration);
    } on Object catch (error) {
      _finishOperation();
      _showMessage('Reconnect failed: $error');
    }
  }

  Future<void> _showExportedLogs() async {
    List<String> files = const [];
    try {
      files = await _nativeVpn.exportLogs();
      if (!mounted) {
        return;
      }
      if (files.isEmpty) {
        _showMessage('No local VPN log files are available.');
        return;
      }
      await showModalBottomSheet<void>(
        context: context,
        builder: (context) => _LogFileList(
          directoryName: path.basename(File(files.first).parent.path),
          files: files,
          onView: _viewLogFile,
        ),
      );
    } on Object catch (error) {
      _showMessage('Could not read local logs: $error');
    } finally {
      final deleted = await deleteExportedLogSnapshots(files);
      if (!deleted) {
        _showMessage('Could not delete every temporary log snapshot.');
      }
    }
  }

  Future<void> _clearLogs() async {
    try {
      await _nativeVpn.clearLogs();
      _showMessage('Local VPN logs were cleared.');
    } on Object catch (error) {
      _showMessage('Could not clear local logs: $error');
    }
  }

  Future<void> _viewLogFile(String filePath) async {
    try {
      final file = File(filePath);
      final raw = await file.readAsString();
      if (!mounted) {
        return;
      }
      final records = _parseLogRecords(raw);
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(path.basename(filePath)),
          content: SizedBox(
            width: 720,
            child: records.isEmpty
                ? const Text('This file is empty.')
                : ListView.separated(
                    shrinkWrap: true,
                    itemCount: records.length,
                    separatorBuilder: (_, __) => const Divider(),
                    itemBuilder: (_, index) => SelectableText(
                      records[index],
                      style: const TextStyle(fontFamily: 'monospace'),
                    ),
                  ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    } on Object catch (error) {
      _showMessage('Could not open the local log: $error');
    }
  }

  static List<String> _parseLogRecords(String raw) {
    final separator = raw.contains('\x1e') ? '\x1e' : '\n';
    return raw
        .split(separator)
        .map((record) => record.trimRight())
        .where((record) => record.isNotEmpty)
        .take(5000)
        .toList(growable: false);
  }

  void _showMessage(String message) {
    if (!mounted) {
      return;
    }
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(message)));
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<VpnStateNotifier>().state;
    final connectionActive = state != VpnState.disconnected;
    return Scaffold(
      appBar: AppBar(title: const Text('TrustTunnel Client')),
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                'State: ${state.name}',
                style: Theme.of(context).textTheme.titleMedium,
              ),
              const SizedBox(height: 8),
              const Text(
                'Paste the endpoint TOML exported by your own TrustTunnel server. '
                'The app does not include a public VPN, public DNS fallback, analytics, or crash reporting.',
              ),
              const SizedBox(height: 12),
              Expanded(
                child: CodeTheme(
                  data: const CodeThemeData(styles: gruvboxDarkTheme),
                  child: CodeField(
                    controller: _configuration,
                    expands: true,
                    maxLines: null,
                  ),
                ),
              ),
              const SizedBox(height: 12),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  FilledButton(
                    onPressed: _operationInProgress ? null : _toggleConnection,
                    child: Text(connectionActive ? 'Disconnect' : 'Connect'),
                  ),
                  OutlinedButton(
                    onPressed: _operationInProgress || !connectionActive
                        ? null
                        : _reconnect,
                    child: const Text('Reconnect'),
                  ),
                  OutlinedButton(
                    onPressed: _showExportedLogs,
                    child: const Text('View Local Logs'),
                  ),
                  TextButton(
                    onPressed: _clearLogs,
                    child: const Text('Clear Local Logs'),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _LogFileList extends StatelessWidget {
  const _LogFileList({
    required this.directoryName,
    required this.files,
    required this.onView,
  });

  final String directoryName;
  final List<String> files;
  final Future<void> Function(String filePath) onView;

  @override
  Widget build(BuildContext context) => SafeArea(
    child: ListView(
      shrinkWrap: true,
      children: [
        ListTile(
          title: const Text('Local VPN log files'),
          subtitle: Text(directoryName),
        ),
        for (final file in files)
          ListTile(
            leading: const Icon(Icons.description_outlined),
            title: Text(path.basename(file)),
            onTap: () async {
              await onView(file);
              if (context.mounted) {
                Navigator.of(context).pop();
              }
            },
          ),
      ],
    ),
  );
}

Future<bool> deleteExportedLogSnapshots(Iterable<String> paths) async {
  var deleted = true;
  final directories = <String>{};
  for (final filePath in paths) {
    final file = File(filePath);
    directories.add(file.parent.path);
    try {
      if (await file.exists()) {
        await file.delete();
      }
    } on FileSystemException {
      deleted = false;
    }
  }
  for (final directoryPath in directories) {
    try {
      final directory = Directory(directoryPath);
      if (await directory.exists()) {
        await directory.delete();
      }
    } on FileSystemException {
      // A non-empty directory may contain a snapshot from another caller.
    }
  }
  return deleted;
}
