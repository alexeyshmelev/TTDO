import TrustTunnelClient
import Foundation

class NativeVpnInterfaceImpl : NativeVpnInterface {
    private let vpnManager: VpnManager
    init(callbacks: FlutterCallbacks) {
        self.vpnManager = VpnManager(
            bundleIdentifier: "org.trusttunnel.client.PacketTunnel",
            appGroup: "group.org.trusttunnel.client",
            stateChangeCallback: { state in
                DispatchQueue.main.async {
                    callbacks.onStateChanged(state: Int64(state)) { _ in }
                }
            },
            connectionInfoCallback: { info in
                DispatchQueue.main.async {
                    callbacks.onConnectionInfo(info: info) { _ in }
                }
            }
        )
    }
    func start(config: String) throws {
        self.vpnManager.start(config: config)
    }
    func stop() throws {
        self.vpnManager.stop()
    }
    func exportLogs() throws -> [String] {
        return vpnManager.exportLogs()
    }
    func clearLogs() throws {
        _ = vpnManager.clearLogs()
    }
}
