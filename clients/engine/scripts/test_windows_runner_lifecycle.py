#!/usr/bin/env python3

import pathlib
import unittest


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]
RUNNER_ROOT = ENGINE_ROOT.parent / "app" / "windows" / "runner"


class WindowsRunnerLifecycleTest(unittest.TestCase):
    def test_native_callback_context_outlives_queued_callbacks(self):
        header = (RUNNER_ROOT / "native_vpn_impl.h").read_text(encoding="utf-8")
        source = (RUNNER_ROOT / "native_vpn_impl.cpp").read_text(encoding="utf-8")

        self.assertIn("std::shared_ptr<CallbackContext>", header)
        self.assertIn("std::enable_shared_from_this<CallbackContext>", source)
        self.assertIn("vpn_easy_stop_and_wait();", source)
        self.assertIn("m_callback_context->active.store(false);", source)
        self.assertNotIn("static_cast<NativeVpnImpl *>(arg)", source)
        self.assertIn("static_cast<CallbackContext *>(arg)", source)

    def test_window_shutdown_detaches_and_drains_callbacks(self):
        source = (RUNNER_ROOT / "flutter_window.cpp").read_text(encoding="utf-8")

        on_destroy = source.index("void FlutterWindow::OnDestroy()")
        detach = source.index("NativeVpnInterface::SetUp(", on_destroy)
        self.assertIn("nullptr", source[detach : source.index(";", detach)])
        shutdown = source.index("native->Shutdown();", detach)
        destroy = source.index("native_interface_ = nullptr;", shutdown)
        drain = source.index("DrainPendingUiTasks();", destroy)
        flutter = source.index("flutter_controller_ = nullptr;", drain)
        self.assertLess(detach, shutdown)
        self.assertLess(shutdown, destroy)
        self.assertLess(destroy, drain)
        self.assertLess(drain, flutter)
        self.assertIn("if (!PostMessageW(", source)
        self.assertIn("delete heap_task;", source)


if __name__ == "__main__":
    unittest.main()
