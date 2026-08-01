#include "native_vpn_impl.h"
#include "vpn/vpn_easy.h"

struct NativeVpnImpl::CallbackContext
    : std::enable_shared_from_this<CallbackContext> {
  CallbackContext(IUIThreadDispatcher *dispatcher, FlutterCallbacks &&callbacks)
      : callbacks(std::move(callbacks)), dispatcher(dispatcher) {}

  void NotifyStateChanged(int state) {
    auto self = shared_from_this();
    dispatcher->RunOnUIThread([self = std::move(self), state]() {
      if (!self->active.load()) {
        return;
      }
      self->callbacks.OnStateChanged(
          state, []() {},
          [self](const FlutterError &error) {
            warnlog(self->logger, "Failed to set updated VPN state: {}:{}",
                    error.code(), error.message());
          });
    });
  }

  ag::Logger logger{"NativeVpnImpl"};
  FlutterCallbacks callbacks;
  IUIThreadDispatcher *dispatcher;
  std::atomic_bool active{true};
};

void NativeVpnImpl::StateChangedHandler(void *arg, int state) {
  auto *ctx = static_cast<CallbackContext *>(arg);
  ctx->NotifyStateChanged(state);
}

NativeVpnImpl::NativeVpnImpl(IUIThreadDispatcher *dispatcher,
                             FlutterCallbacks &&callbacks)
    : m_callback_context(
          std::make_shared<CallbackContext>(dispatcher, std::move(callbacks))) {
}

NativeVpnImpl::~NativeVpnImpl() { Shutdown(); }

std::optional<FlutterError> NativeVpnImpl::Start(const std::string &config) {
  vpn_easy_start(config.c_str(), StateChangedHandler, m_callback_context.get());
  return std::nullopt;
}

std::optional<FlutterError> NativeVpnImpl::Stop() {
  vpn_easy_stop();
  return std::nullopt;
}

ErrorOr<flutter::EncodableList> NativeVpnImpl::ExportLogs() {
  // Log export is not yet implemented for the Windows adapter
  return flutter::EncodableList{};
}

std::optional<FlutterError> NativeVpnImpl::ClearLogs() {
  // Log clearing is not yet implemented for the Windows adapter
  return std::nullopt;
}

void NativeVpnImpl::Shutdown() {
  if (m_shutdown.exchange(true)) {
    return;
  }
  vpn_easy_stop_and_wait();
  m_callback_context->active.store(false);
}
