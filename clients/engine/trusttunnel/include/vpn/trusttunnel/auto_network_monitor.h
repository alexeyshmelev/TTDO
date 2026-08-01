#pragma once

#include <atomic>
#include <memory>

#include <common/network_monitor.h>

#include "vpn/trusttunnel/client.h"

namespace ag {
#ifdef _WIN32
struct AutoNetworkMonitorWindowsCallbacks;
#endif

/**
 * Automatic network monitoring.
 *
 * Monitors the active network interface and network availability, calls
 * `TrustTunnelClient::notify_network_change` and `vpn_network_manager_set_outbound_interface` respectively.
 * Respects the forced network interface if `bound_if` is not empty.
 */
class AutoNetworkMonitor {
public:
    explicit AutoNetworkMonitor(TrustTunnelClient *client, std::string bound_if);
    ~AutoNetworkMonitor();

    bool start();
    void stop();

private:
#ifdef _WIN32
    friend struct AutoNetworkMonitorWindowsCallbacks;

    void schedule_windows_refresh();
    void refresh_windows_network_state();

    void *m_windows_interface_notification = nullptr;
    void *m_windows_route_notification = nullptr;
    std::atomic_bool m_windows_stopping = false;
    std::atomic_bool m_windows_refresh_pending = false;
#endif

    TrustTunnelClient *m_client = nullptr;
    std::string m_bound_if;
    std::unique_ptr<ag::utils::NetworkMonitor> m_network_monitor;
    UniquePtr<VpnEventLoop, &vpn_event_loop_destroy> m_network_monitor_loop = nullptr;
    std::thread m_network_monitor_loop_thread;
};
} // namespace ag
