#pragma once

#include "vpn/trusttunnel/config.h"

#include <common/logger.h>
#include <common/net_utils.h>
#include <cxxopts.hpp>

#include <cstdio>
#include <optional>
#include <string_view>

namespace ag {

class TrustTunnelCliUtils {
public:
    static std::optional<ag::LogLevel> parse_loglevel(std::string_view level);

    static bool apply_cmd_args(TrustTunnelConfig &config, const cxxopts::ParseResult &args);
};

FILE *open_private_log_file(std::string_view filename);

SystemDnsServers parse_resolv_conf(std::string_view contents);
} // namespace ag
