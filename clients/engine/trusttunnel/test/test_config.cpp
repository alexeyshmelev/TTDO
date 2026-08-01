#include <gtest/gtest.h>

#include "common/logger.h"
#include "utils.h"
#include "vpn/trusttunnel/config.h"

#ifndef _WIN32
#include <filesystem>
#include <fstream>
#include <sys/stat.h>
#include <unistd.h>
#endif

using namespace ag;

TEST(TrustTunnelConfigLogTest, MissingListenerDoesNotLogEndpointCredentials) {
    constexpr std::string_view SECRET = "endpoint-password-must-not-be-logged";
    auto parsed = toml::parse(R"(
vpn_mode = "general"

[endpoint]
hostname = "vpn.example.invalid"
addresses = ["192.0.2.10:443"]
username = "test-user"
password = "endpoint-password-must-not-be-logged"
upstream_protocol = "http2"
)");

    std::string log_output;
    LogLevel previous_level = Logger::get_log_level();
    Logger::set_log_level(LOG_LEVEL_TRACE);
    Logger::set_callback([&log_output](LogLevel, std::string_view message) {
        log_output.append(message);
        log_output.push_back('\n');
    });

    std::optional config = TrustTunnelConfig::build_config(parsed.table());

    Logger::set_callback(Logger::LOG_TO_STDERR);
    Logger::set_log_level(previous_level);

    EXPECT_FALSE(config.has_value());
    EXPECT_NE(log_output.find("Listener configuration is not a table"), std::string::npos);
    EXPECT_EQ(log_output.find(SECRET), std::string::npos);
}

TEST(ResolvConfTest, RetainsLoopbackStubWithoutInventingFallbacks) {
    SystemDnsServers servers = parse_resolv_conf(R"(
# systemd-resolved local stub
nameserver 127.0.0.53
nameserver ::1 # local IPv6 resolver
nameserver malformed
nameserver 127.0.0.53
search example.invalid
)");

    ASSERT_EQ(servers.main.size(), 2);
    EXPECT_EQ(servers.main[0].address, "127.0.0.53");
    EXPECT_EQ(servers.main[1].address, "::1");
    EXPECT_TRUE(servers.fallback.empty());
    EXPECT_TRUE(servers.bootstrap.empty());
}

TEST(ResolvConfTest, EmptyInputStaysEmpty) {
    SystemDnsServers servers = parse_resolv_conf("");
    EXPECT_TRUE(servers.main.empty());
    EXPECT_TRUE(servers.fallback.empty());
    EXPECT_TRUE(servers.bootstrap.empty());
}

TEST(ResolvConfTest, SkipsInternalUnfilteredDnsMarkers) {
    SystemDnsServers servers = parse_resolv_conf(R"(
nameserver 46.243.231.30
nameserver 46.243.231.31
nameserver 2a10:50c0::1:ff
nameserver 2a10:50c0::2:ff
nameserver 127.0.0.53
nameserver 192.0.2.53
)");

    ASSERT_EQ(servers.main.size(), 2);
    EXPECT_EQ(servers.main[0].address, "127.0.0.53");
    EXPECT_EQ(servers.main[1].address, "192.0.2.53");
}

#ifndef _WIN32
TEST(TrustTunnelLogFileTest, RestrictsPermissionsAndRejectsSymbolicLinks) {
    namespace fs = std::filesystem;
    fs::path directory = fs::temp_directory_path() / ("trusttunnel-log-test-" + std::to_string(::getpid()));
    fs::remove_all(directory);
    ASSERT_TRUE(fs::create_directory(directory));
    fs::path path = directory / "client.log";
    {
        std::ofstream existing(path);
        existing << "old log";
    }
    fs::permissions(path, fs::perms::all);

    FILE *file = open_private_log_file(path.string());
    ASSERT_NE(file, nullptr);
    std::fclose(file);
    EXPECT_EQ(fs::status(path).permissions() & fs::perms::all, fs::perms::owner_read | fs::perms::owner_write);

    fs::path target = directory / "target.log";
    fs::path link = directory / "linked.log";
    {
        std::ofstream target_file(target);
        target_file << "keep";
    }
    fs::create_symlink(target, link);
    EXPECT_EQ(open_private_log_file(link.string()), nullptr);
    std::ifstream target_file(target);
    std::string target_contents;
    target_file >> target_contents;
    EXPECT_EQ(target_contents, "keep");

    fs::path fifo = directory / "blocked.log";
    ASSERT_EQ(::mkfifo(fifo.c_str(), S_IRUSR | S_IWUSR), 0);
    EXPECT_EQ(open_private_log_file(fifo.string()), nullptr);
    EXPECT_EQ(open_private_log_file("/dev/null"), nullptr);

    fs::remove_all(directory);
}
#endif
