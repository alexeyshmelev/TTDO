#ifdef __APPLE__
#include <net/if.h>
#include <netinet/in.h>
#endif // __APPLE__

#ifdef __linux__
// clang-format off
#include <net/if.h>

#include <linux/if.h>
#include <linux/if_tun.h>
// clang-format on
#endif // __linux__

#ifdef _WIN32
#include <WinSock2.h>
#include <iphlpapi.h>
#endif // _WIN32

#include <optional>

#include "common/logger.h"
#include "net/network_manager.h"
#include "vpn/trusttunnel/auto_network_monitor.h"

namespace ag {

#ifdef _WIN32
static const Logger g_logger{"AUTO_NETWORK_MONITOR"};

struct AutoNetworkMonitorWindowsCallbacks {
    static void CALLBACK on_interface_change(PVOID context, PMIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE) {
        static_cast<AutoNetworkMonitor *>(context)->schedule_windows_refresh();
    }

    static void CALLBACK on_route_change(PVOID context, PMIB_IPFORWARD_ROW2, MIB_NOTIFICATION_TYPE) {
        static_cast<AutoNetworkMonitor *>(context)->schedule_windows_refresh();
    }
};
#endif

AutoNetworkMonitor::AutoNetworkMonitor(TrustTunnelClient *client, std::string bound_if)
        : m_client(client)
        , m_bound_if(std::move(bound_if)) {
}

AutoNetworkMonitor::~AutoNetworkMonitor() {
    stop();
}

static std::optional<uint32_t> find_interface(std::string_view if_name) {
    uint32_t if_index = if_nametoindex(if_name.data());
    if (if_index != 0) {
        return if_index;
    }
#ifdef _WIN32
    if (auto idx = ag::utils::to_integer<uint32_t>(if_name)) {
        return *idx;
    }
#endif
    return std::nullopt;
}

static bool update_interface(std::string_view if_name) {
    std::optional<uint32_t> if_index = find_interface(if_name);
    if (!if_index.has_value()) {
        return false;
    }
    vpn_network_manager_set_outbound_interface(*if_index);
    return true;
}

#ifdef _WIN32
void AutoNetworkMonitor::schedule_windows_refresh() {
    if (m_windows_stopping.load(std::memory_order_acquire) || m_windows_refresh_pending.exchange(true)) {
        return;
    }

    event_loop::AutoTaskId task = event_loop::submit(m_network_monitor_loop.get(), [this] {
        m_windows_refresh_pending.store(false);
        if (!m_windows_stopping.load(std::memory_order_acquire)) {
            refresh_windows_network_state();
        }
    });
    if (task.has_value()) {
        task.release();
    } else {
        m_windows_refresh_pending.store(false);
    }
}

void AutoNetworkMonitor::refresh_windows_network_state() {
    std::string if_name = m_bound_if.empty() ? m_network_monitor->get_default_interface() : m_bound_if;
    std::optional<uint32_t> if_index = find_interface(if_name);
    bool is_connected = if_index.has_value();
    if (is_connected && !m_bound_if.empty()) {
        MIB_IF_ROW2 row{};
        row.InterfaceIndex = *if_index;
        is_connected = GetIfEntry2(&row) == NO_ERROR && row.OperStatus == IfOperStatusUp;
    }

    if (is_connected) {
        vpn_network_manager_set_outbound_interface(*if_index);
        if (auto error = TrustTunnelClient::set_system_dns(*if_index)) {
            warnlog(g_logger, "Failed to refresh system DNS servers: {}", error->pretty_str());
            vpn_network_manager_update_system_dns({});
            m_client->notify_network_change(VPN_NS_NOT_CONNECTED);
            return;
        }
    } else {
        vpn_network_manager_set_outbound_interface(0);
        vpn_network_manager_update_system_dns({});
    }
    m_client->notify_network_change(is_connected ? VPN_NS_CONNECTED : VPN_NS_NOT_CONNECTED);
}
#endif

bool AutoNetworkMonitor::start() {
    m_network_monitor_loop.reset(vpn_event_loop_create());
    m_network_monitor_loop_thread = std::thread([this]() {
        vpn_event_loop_run(m_network_monitor_loop.get());
    });

    bool is_bound_if_override = !m_bound_if.empty();

    m_network_monitor = ag::utils::create_network_monitor(
            [this, is_bound_if_override](const std::string &if_name, bool is_connected) {
                std::string_view selected_interface = is_bound_if_override ? m_bound_if : if_name;
                std::optional<uint32_t> if_index = find_interface(selected_interface);
                if (!is_bound_if_override && if_index.has_value()) {
                    vpn_network_manager_set_outbound_interface(*if_index);
                }
#ifdef _WIN32
                if (is_connected && if_index.has_value()) {
                    if (auto error = TrustTunnelClient::set_system_dns(*if_index)) {
                        warnlog(g_logger, "Failed to refresh system DNS servers: {}", error->pretty_str());
                    }
                }
#endif
                m_client->notify_network_change(is_connected ? ag::VPN_NS_CONNECTED : ag::VPN_NS_NOT_CONNECTED);
            });

#ifdef _WIN32
    m_windows_stopping.store(false);
    HANDLE interface_notification = nullptr;
    DWORD result = NotifyIpInterfaceChange(
            AF_UNSPEC, AutoNetworkMonitorWindowsCallbacks::on_interface_change, this, FALSE, &interface_notification);
    if (result != NO_ERROR) {
        errlog(g_logger, "Failed to register IP interface notifications: {}", result);
        return false;
    }
    m_windows_interface_notification = interface_notification;

    HANDLE route_notification = nullptr;
    result = NotifyRouteChange2(
            AF_UNSPEC, AutoNetworkMonitorWindowsCallbacks::on_route_change, this, FALSE, &route_notification);
    if (result != NO_ERROR) {
        errlog(g_logger, "Failed to register route notifications: {}", result);
        return false;
    }
    m_windows_route_notification = route_notification;
#endif

    if (is_bound_if_override && !update_interface(m_bound_if)) {
        return false;
    }

    event_loop::dispatch_sync(m_network_monitor_loop.get(), [this, is_bound_if_override]() {
#ifndef _WIN32
        m_network_monitor->start(vpn_event_loop_get_base(m_network_monitor_loop.get()));
        if (!is_bound_if_override) {
            auto if_name = m_network_monitor->get_default_interface();
            update_interface(if_name);
        }
#else
        (void) is_bound_if_override;
        refresh_windows_network_state();
#endif
    });

    return true;
}

void AutoNetworkMonitor::stop() {
    if (m_network_monitor_loop) {
#ifdef _WIN32
        m_windows_stopping.store(true, std::memory_order_release);
        auto cancel_notification = [](void *&notification) {
            if (notification != nullptr) {
                DWORD result = CancelMibChangeNotify2(notification);
                if (result != NO_ERROR) {
                    warnlog(g_logger, "Failed to cancel network notification: {}", result);
                }
                notification = nullptr;
            }
        };
        cancel_notification(m_windows_interface_notification);
        cancel_notification(m_windows_route_notification);
#endif
        event_loop::dispatch_sync(m_network_monitor_loop.get(), [this]() {
#ifndef _WIN32
            m_network_monitor->stop();
#endif
        });
        vpn_event_loop_stop(m_network_monitor_loop.get());
        if (m_network_monitor_loop_thread.joinable()) {
            m_network_monitor_loop_thread.join();
        }
        m_network_monitor_loop.reset();
#ifdef _WIN32
        m_windows_refresh_pending.store(false);
#endif
    }
}

} // namespace ag
