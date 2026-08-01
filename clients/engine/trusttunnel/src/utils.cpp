#include "utils.h"

#include <algorithm>
#include <cerrno>
#include <magic_enum/magic_enum.hpp>
#include <sstream>

#include "common/net_utils.h"
#include "net/utils.h"

#ifndef _WIN32
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>
#endif

namespace ag {

static Logger g_logger("TrustTunnelCliUtils");

std::optional<ag::LogLevel> TrustTunnelCliUtils::parse_loglevel(std::string_view level) {
    static const std::unordered_map<std::string_view, ag::LogLevel> LOG_LEVEL_MAP = {
            {"error", ag::LOG_LEVEL_ERROR},
            {"warn", ag::LOG_LEVEL_WARN},
            {"info", ag::LOG_LEVEL_INFO},
            {"debug", ag::LOG_LEVEL_DEBUG},
            {"trace", ag::LOG_LEVEL_TRACE},
    };

    if (auto it = LOG_LEVEL_MAP.find(level); it != LOG_LEVEL_MAP.end()) {
        return it->second;
    }
    return std::nullopt;
}

bool TrustTunnelCliUtils::apply_cmd_args(TrustTunnelConfig &config, const cxxopts::ParseResult &args) {
    if (args.count("s") > 0) {
        bool x = args["s"].as<bool>();
        if (x != config.location.skip_verification) {
            infolog(g_logger, "Skip verification value was overwritten: old={}, new={}",
                    config.location.skip_verification, x);
        }
        config.location.skip_verification = x;
    }
    if (args.count("loglevel") > 0) {
        if (auto loglevel = parse_loglevel(args["loglevel"].as<std::string>())) {
            if (loglevel != config.loglevel) {
                infolog(g_logger, "Log Level value was overwritten: old={}, new={}",
                        magic_enum::enum_name(config.loglevel), magic_enum::enum_name(*loglevel));
                config.loglevel = *loglevel;
            }
        } else {
            errlog(g_logger, "Unexpected log level: {}", args["loglevel"].as<std::string>());
            return false;
        }
    }
    return true;
}

FILE *open_private_log_file(std::string_view filename) {
    std::string path(filename);
#ifdef _WIN32
    return std::fopen(path.c_str(), "w");
#else
    int fd = ::open(path.c_str(), O_WRONLY | O_CREAT | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK, S_IRUSR | S_IWUSR);
    if (fd < 0) {
        return nullptr;
    }
    struct stat file_status{};
    if (::fstat(fd, &file_status) != 0) {
        int error = errno;
        ::close(fd);
        errno = error;
        return nullptr;
    }
    if (!S_ISREG(file_status.st_mode)) {
        ::close(fd);
        errno = EINVAL;
        return nullptr;
    }
    if (::ftruncate(fd, 0) != 0 || ::fchmod(fd, S_IRUSR | S_IWUSR) != 0) {
        int error = errno;
        ::close(fd);
        errno = error;
        return nullptr;
    }
    FILE *file = ::fdopen(fd, "w");
    if (file == nullptr) {
        int error = errno;
        ::close(fd);
        errno = error;
    }
    return file;
#endif
}

SystemDnsServers parse_resolv_conf(std::string_view contents) {
    SystemDnsServers servers;
    std::istringstream input{std::string{contents}};
    for (std::string line; std::getline(input, line);) {
        std::istringstream fields{line};
        std::string directive;
        std::string address;
        if (!(fields >> directive >> address) || directive != "nameserver") {
            continue;
        }
        SocketAddress parsed(address, utils::PLAIN_DNS_PORT_NUMBER);
        if (!parsed.valid()) {
            continue;
        }
        std::string normalized = parsed.host_str();
        if (std::ranges::find(utils::AG_UNFILTERED_DNS_IPS_V4, normalized) != std::end(utils::AG_UNFILTERED_DNS_IPS_V4)
                || std::ranges::find(utils::AG_UNFILTERED_DNS_IPS_V6, normalized)
                        != std::end(utils::AG_UNFILTERED_DNS_IPS_V6)) {
            continue;
        }
        if (std::ranges::none_of(servers.main, [&normalized](const SystemDnsServer &server) {
                return server.address == normalized;
            })) {
            servers.main.emplace_back(SystemDnsServer{.address = std::move(normalized)});
        }
    }
    return servers;
}

} // namespace ag
