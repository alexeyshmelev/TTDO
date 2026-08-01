#!/usr/bin/env python3

import pathlib
import re
import unittest


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]
VPN_MANAGER = (
    ENGINE_ROOT
    / "platform"
    / "apple"
    / "TrustTunnelClient"
    / "VpnManager.swift"
)
PACKET_TUNNEL_PROVIDER = (
    ENGINE_ROOT
    / "platform"
    / "apple"
    / "TrustTunnelClient"
    / "PacketTunnelProvider.swift"
)


class AppleStartCallbackOrderTest(unittest.TestCase):
    def test_configuration_reload_suppresses_pre_start_disconnected_state(self):
        source = VPN_MANAGER.read_text(encoding="utf-8")
        update = re.search(
            r"private func updateConfiguration\(manager:.*?\n    }\n\n"
            r"    private func deleteConfiguration",
            source,
            re.DOTALL,
        )

        self.assertIsNotNone(update)
        self.assertIn(
            "startObservingStatus(manager: manager, reportInitialStatus: false)",
            update.group(0),
        )
        self.assertIn("if reportInitialStatus {", source)
        self.assertIn('logCurrentStatus(prefix: "initial", manager: manager)', source)

    def test_packet_tunnel_start_completion_is_one_shot(self):
        source = PACKET_TUNNEL_PROVIDER.read_text(encoding="utf-8")
        start = re.search(
            r"open override func startTunnel\(.*?\n    }\n\n"
            r"    @discardableResult",
            source,
            re.DOTALL,
        )
        helper = re.search(
            r"private func completeStart\(.*?\n    }\n\n"
            r"    open override func stopTunnel",
            source,
            re.DOTALL,
        )

        self.assertIsNotNone(start)
        self.assertIsNotNone(helper)
        self.assertEqual(start.group(0).count("self.completeStart("), 9)
        self.assertNotIn("completionHandler(", start.group(0))
        self.assertLess(
            start.group(0).index("VpnClient.prepareSystemDns()"),
            start.group(0).index("setTunnelNetworkSettings"),
        )
        self.assertLess(
            start.group(0).index("prepareEndpointAddresses()"),
            start.group(0).index("setTunnelNetworkSettings"),
        )
        self.assertLess(
            start.group(0).index("VpnClient.prepareSystemDns()"),
            start.group(0).index("prepareEndpointAddresses()"),
        )
        self.assertLess(
            helper.group(0).index("self.startProcessed = true"),
            helper.group(0).index("completionHandler(error)"),
        )


if __name__ == "__main__":
    unittest.main()
