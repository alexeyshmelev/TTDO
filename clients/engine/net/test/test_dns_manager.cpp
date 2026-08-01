#include <gtest/gtest.h>

#include <string>
#include <utility>
#include <vector>

#include "net/dns_manager.h"

TEST(DnsManager, RemovesInternalMarkersWithoutAddingPublicFallbacks) {
    ag::DnsManager *manager = ag::dns_manager_create();
    ag::SystemDnsServers servers;
    servers.main = {
            {.address = std::string{ag::VPN_INTERNAL_DNS_IPS_V4[0]}},
            {.address = "127.0.0.53"},
            {.address = "203.0.113.53"},
    };
    servers.fallback = {
            std::string{ag::VPN_INTERNAL_DNS_IPS_V6[0]},
            "203.0.113.54",
    };
    servers.bootstrap = {"127.0.0.1", "203.0.113.55"};

    ASSERT_TRUE(ag::dns_manager_set_system_servers(manager, std::move(servers)));
    ag::SystemDnsServers actual = ag::dns_manager_get_system_servers(manager);

    ASSERT_EQ(actual.main.size(), 2);
    EXPECT_EQ(actual.main[0].address, "127.0.0.53");
    EXPECT_EQ(actual.main[1].address, "203.0.113.53");
    EXPECT_EQ(actual.fallback, std::vector<std::string>{"203.0.113.54"});
    EXPECT_EQ(actual.bootstrap, (std::vector<std::string>{"127.0.0.1", "203.0.113.55"}));

    ag::dns_manager_destroy(manager);
}

TEST(DnsManager, LeavesNoFallbackWhenSystemDnsIsEmpty) {
    ag::DnsManager *manager = ag::dns_manager_create();

    ASSERT_TRUE(ag::dns_manager_set_system_servers(manager, {}));
    ag::SystemDnsServers actual = ag::dns_manager_get_system_servers(manager);

    EXPECT_TRUE(actual.main.empty());
    EXPECT_TRUE(actual.fallback.empty());
    EXPECT_TRUE(actual.bootstrap.empty());

    ag::dns_manager_destroy(manager);
}

TEST(DnsManager, RetainsCapturedServersWhenOnlyInternalMarkersAreReported) {
    ag::DnsManager *manager = ag::dns_manager_create();
    ag::SystemDnsServers captured{.main = {{.address = "192.0.2.53"}}};
    ASSERT_TRUE(ag::dns_manager_set_system_servers(manager, captured));

    ag::SystemDnsServers markers{
            .main = {{.address = std::string{ag::VPN_INTERNAL_DNS_IPS_V4[0]}}},
            .fallback = {std::string{ag::VPN_INTERNAL_DNS_IPS_V6[0]}},
    };
    ASSERT_TRUE(ag::dns_manager_set_system_servers(manager, std::move(markers)));

    ag::SystemDnsServers actual = ag::dns_manager_get_system_servers(manager);
    ASSERT_EQ(actual.main.size(), 1);
    EXPECT_EQ(actual.main[0].address, "192.0.2.53");

    ag::dns_manager_destroy(manager);
}

TEST(DnsManager, ExplicitEmptyUpdateClearsCapturedServers) {
    ag::DnsManager *manager = ag::dns_manager_create();
    ASSERT_TRUE(ag::dns_manager_set_system_servers(manager, {.main = {{.address = "192.0.2.53"}}}));

    ASSERT_TRUE(ag::dns_manager_set_system_servers(manager, {}));

    ag::SystemDnsServers actual = ag::dns_manager_get_system_servers(manager);
    EXPECT_TRUE(actual.main.empty());
    EXPECT_TRUE(actual.fallback.empty());
    EXPECT_TRUE(actual.bootstrap.empty());

    ag::dns_manager_destroy(manager);
}
