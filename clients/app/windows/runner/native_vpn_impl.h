#pragma once
#include <common/logger.h>

#include <atomic>
#include <memory>

#include "pigeon/native_communication.h"
#include "ui_thread_dispatcher.h"

class NativeVpnImpl : public NativeVpnInterface {
public:
  NativeVpnImpl(IUIThreadDispatcher *dispatcher, FlutterCallbacks &&callbacks);
  ~NativeVpnImpl() override;
  std::optional<FlutterError> Start(const std::string &config) override;
  std::optional<FlutterError> Stop() override;
  ErrorOr<flutter::EncodableList> ExportLogs() override;
  std::optional<FlutterError> ClearLogs() override;
  void Shutdown();

private:
  struct CallbackContext;
  static void StateChangedHandler(void *arg, int state);

  std::shared_ptr<CallbackContext> m_callback_context;
  std::atomic_bool m_shutdown{false};
};
