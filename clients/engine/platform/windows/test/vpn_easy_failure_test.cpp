#include "vpn/vpn.h"
#include "vpn/vpn_easy.h"

#include <chrono>
#include <condition_variable>
#include <mutex>
#include <optional>
#include <thread>

#include <gtest/gtest.h>

struct CallbackState {
    std::mutex mutex;
    std::condition_variable changed;
    std::optional<int> state;
    int callback_count = 0;
};

static void capture_state(void *arg, int state) {
    auto *capture = static_cast<CallbackState *>(arg);
    {
        std::lock_guard guard(capture->mutex);
        capture->state = state;
        ++capture->callback_count;
    }
    capture->changed.notify_one();
}

TEST(VpnEasyTest, FailedStartReportsDisconnected) {
    CallbackState capture;

    vpn_easy_start("[", capture_state, &capture);

    std::unique_lock lock(capture.mutex);
    ASSERT_TRUE(capture.changed.wait_for(lock, std::chrono::seconds(5), [&capture] {
        return capture.state.has_value();
    }));
    EXPECT_EQ(ag::VPN_SS_DISCONNECTED, *capture.state);
}

TEST(VpnEasyTest, StopAndWaitDrainsQueuedStartCallbacks) {
    CallbackState capture;

    vpn_easy_start("[", capture_state, &capture);
    vpn_easy_stop_and_wait();

    int callbacks_after_return;
    {
        std::lock_guard lock(capture.mutex);
        ASSERT_TRUE(capture.state.has_value());
        EXPECT_EQ(ag::VPN_SS_DISCONNECTED, *capture.state);
        EXPECT_GT(capture.callback_count, 0);
        callbacks_after_return = capture.callback_count;
    }

    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    std::lock_guard lock(capture.mutex);
    EXPECT_EQ(callbacks_after_return, capture.callback_count);
}
