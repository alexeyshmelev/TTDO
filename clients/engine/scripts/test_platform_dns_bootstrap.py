#!/usr/bin/env python3

import pathlib
import unittest


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]
APPLE_WRAPPER = ENGINE_ROOT / "platform" / "apple" / "VpnClient" / "VpnClient.mm"
WINDOWS_WRAPPER = ENGINE_ROOT / "platform" / "windows" / "src" / "vpn_easy.cpp"
NETWORK_MONITOR = ENGINE_ROOT / "trusttunnel" / "src" / "auto_network_monitor.cpp"
TRUSTTUNNEL_CMAKE = ENGINE_ROOT / "trusttunnel" / "CMakeLists.txt"
APPLE_PROVIDER = (
    ENGINE_ROOT
    / "platform"
    / "apple"
    / "TrustTunnelClient"
    / "PacketTunnelProvider.swift"
)
APPLE_CONFIG = (
    ENGINE_ROOT
    / "platform"
    / "apple"
    / "TrustTunnelClient"
    / "VpnConfig.swift"
)
CLIENT = ENGINE_ROOT / "trusttunnel" / "src" / "client.cpp"


class PlatformDnsBootstrapTest(unittest.TestCase):
    def test_apple_wrapper_uses_system_dns_collector(self):
        source = APPLE_WRAPPER.read_text(encoding="utf-8")

        self.assertIn("+ (bool)prepareSystemDns", source)
        self.assertIn("ag::TrustTunnelClient::set_system_dns()", source)

    def test_windows_collects_dns_before_connect(self):
        source = WINDOWS_WRAPPER.read_text(encoding="utf-8")

        collect = source.index("ag::TrustTunnelClient::set_system_dns()")
        connect = source.index("vpn->client->connect(", collect)
        self.assertLess(collect, connect)

    def test_windows_refreshes_dns_before_notifying_network_change(self):
        source = NETWORK_MONITOR.read_text(encoding="utf-8")
        cmake = TRUSTTUNNEL_CMAKE.read_text(encoding="utf-8")

        self.assertIn("NotifyIpInterfaceChange", source)
        self.assertIn("NotifyRouteChange2", source)
        self.assertIn("cancel_notification(m_windows_interface_notification)", source)
        self.assertIn("cancel_notification(m_windows_route_notification)", source)
        self.assertIn("target_link_libraries(vpnlibs_trusttunnel Iphlpapi)", cmake)
        refresh = source.index("TrustTunnelClient::set_system_dns(*if_index)")
        clear = source.index("vpn_network_manager_update_system_dns({})", refresh)
        disconnected = source.index("VPN_NS_NOT_CONNECTED", clear)
        notify = source.index("m_client->notify_network_change", refresh)
        self.assertLess(refresh, clear)
        self.assertLess(clear, notify)
        self.assertLess(notify, disconnected)

    def test_apple_installs_internal_dns_only_for_explicit_upstreams(self):
        source = APPLE_PROVIDER.read_text(encoding="utf-8")
        model = APPLE_CONFIG.read_text(encoding="utf-8")

        self.assertIn(
            "vpnConfig.endpoint.dns_upstreams ?? vpnConfig.dns_upstreams ?? []",
            source,
        )
        self.assertIn("if !effectiveDnsUpstreams.isEmpty", source)
        self.assertIn("networkSettings.dnsSettings = dnsSettings", source)
        self.assertIn("let dns_upstreams: [String]?", model)

    def test_desktop_installs_internal_dns_only_for_explicit_upstreams(self):
        source = CLIENT.read_text(encoding="utf-8")

        self.assertIn("config.change_system_dns && !effective_dns.empty()", source)


if __name__ == "__main__":
    unittest.main()
