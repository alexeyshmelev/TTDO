#!/usr/bin/env python3

import pathlib
import unittest


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]
APPLE_WRAPPER = ENGINE_ROOT / "platform" / "apple" / "VpnClient" / "VpnClient.mm"
WINDOWS_WRAPPER = ENGINE_ROOT / "platform" / "windows" / "src" / "vpn_easy.cpp"


class PlatformClientLifecycleTest(unittest.TestCase):
    def test_apple_output_callback_captures_client_weakly(self):
        source = APPLE_WRAPPER.read_text(encoding="utf-8")
        initializer = source[
            source.index("- (instancetype)initWithConfig:") : source.index("- (void)dealloc")
        ]
        output_handler = initializer[
            initializer.index(".client_output_handler") : initializer.index(
                ".state_changed_handler"
            )
        ]

        self.assertIn("__weak typeof(self) weakSelf = self;", initializer)
        self.assertIn("[weakSelf](ag::VpnClientOutputEvent *event)", output_handler)
        self.assertNotIn("[self](ag::VpnClientOutputEvent *event)", output_handler)
        self.assertIn(
            "__strong typeof(weakSelf) strongSelf = weakSelf;", output_handler
        )
        self.assertIn("strongSelf->_tunnelFlow", output_handler)

    def test_apple_shutdown_handles_partial_init_and_stops_monitor_first(self):
        source = APPLE_WRAPPER.read_text(encoding="utf-8")
        dealloc = source[source.index("- (void)dealloc") : source.index("- (bool)start:")]
        stop = source[source.index("- (bool)stop") : source.index("- (void)notify_sleep")]

        self.assertIn("if (_network_monitor)", dealloc)
        self.assertIn("_network_monitor->stop();", dealloc)
        self.assertLess(
            stop.index("_network_monitor->stop();"),
            stop.index("_native_client->disconnect()"),
        )

    def test_windows_shutdown_stops_monitor_before_disconnect(self):
        source = WINDOWS_WRAPPER.read_text(encoding="utf-8")
        stop = source[
            source.index("void vpn_easy_stop_ex") : source.index("class VpnEasyManager")
        ]

        self.assertLess(
            stop.index("vpn->network_monitor->stop();"),
            stop.index("vpn->client->disconnect();"),
        )


if __name__ == "__main__":
    unittest.main()
