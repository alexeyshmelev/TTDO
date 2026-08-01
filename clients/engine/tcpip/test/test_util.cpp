#include <stdio.h>
#include <string.h>

#include <string>

#ifndef _WIN32
#include <filesystem>
#include <fstream>
#include <sys/stat.h>
#include <unistd.h>
#endif

#include <event2/util.h>
#include <lwip/ip_addr.h>

#include "common/socket_address.h"
#include "tcpip_util.h"
#include "vpn/utils.h"

using namespace ag;

#define ASSERT(x)                                                                                                      \
    do {                                                                                                               \
        if (!(x)) {                                                                                                    \
            fprintf(stderr, "FAILED: %s", #x);                                                                         \
            abort();                                                                                                   \
        }                                                                                                              \
    } while (0)

void test_socket_address_to_ip_addr() {
    // Test IPv4
    {
        // Set
        struct sockaddr_in in4;
        in4.sin_family = AF_INET;
        ASSERT(inet_pton(AF_INET, "192.0.2.1", &in4.sin_addr) > 0);
        in4.sin_port = htons(12344);

        // Convert
        ip_addr_t ip4;
        uint16_t ip4_port;
        socket_address_to_ip_addr(SocketAddress((const sockaddr *) &in4), &ip4, &ip4_port);

        // Check
        ip_addr_t ip4_check;
        ASSERT(ipaddr_aton("192.0.2.1", &ip4_check));
        ASSERT(ip_addr_cmp(&ip4, &ip4_check));
        ASSERT(ip4_port == 12344);
    }

    // Test IPv6
    {
        // Set
        struct sockaddr_in6 in6;
        in6.sin6_family = AF_INET6;
        ASSERT(inet_pton(AF_INET6, "2001:db8:2017::1", &in6.sin6_addr) > 0);
        in6.sin6_port = htons(12346);

        // Convert
        ip_addr_t ip6;
        uint16_t ip6_port;
        socket_address_to_ip_addr(SocketAddress((const struct sockaddr *) &in6), &ip6, &ip6_port);

        // Check
        ip_addr_t ip6_check;
        ASSERT(ipaddr_aton("2001:db8:2017::1", &ip6_check));
        ASSERT(ip_addr_cmp(&ip6, &ip6_check));
        ASSERT(ip6_port == 12346);
    }
}

void test_ip_addr_to_socket_address() {
    // Test IPv4
    {
        // Set
        ip_addr_t ip4;
        constexpr uint16_t ip4_port = 12334;
        ASSERT(ipaddr_aton("192.0.2.2", &ip4));

        // Convert
        SocketAddress in4 = ip_addr_to_socket_address(&ip4, ip4_port);
        socklen_t in4_len = in4.c_socklen();

        // Check
        ASSERT(in4.is_ipv4());
        ASSERT(in4_len == sizeof(struct sockaddr_in));
        struct sockaddr_in *pin = (struct sockaddr_in *) &in4;
        ASSERT(pin->sin_port == PP_HTONS(12334));
        struct in_addr sin_addr_check;
        ASSERT(inet_pton(AF_INET, "192.0.2.2", &sin_addr_check) > 0);
        ASSERT(!memcmp(&pin->sin_addr, &sin_addr_check, sizeof(struct in_addr)));
    }

    // Test IPv6
    {
        // Set
        ip_addr_t ip6;
        constexpr uint16_t ip6_port = 12335;
        ASSERT(ipaddr_aton("2001:db8:2017::2", &ip6));

        // Convert
        SocketAddress in6 = ip_addr_to_socket_address(&ip6, ip6_port);
        socklen_t in6_len = in6.c_socklen();

        // Check
        ASSERT(in6.is_ipv6());
        ASSERT(in6_len == sizeof(struct sockaddr_in6));
        struct sockaddr_in6 *pin6 = (struct sockaddr_in6 *) &in6;
        ASSERT(pin6->sin6_port == PP_HTONS(12335));
        struct in6_addr sin6_addr_check;
        ASSERT(inet_pton(AF_INET6, "2001:db8:2017::2", &sin6_addr_check) > 0);
        ASSERT(!memcmp(&pin6->sin6_addr, &sin6_addr_check, sizeof(struct in6_addr)));
    }
}

#ifndef _WIN32
void test_private_pcap_file() {
    namespace fs = std::filesystem;
    fs::path directory = fs::temp_directory_path() / ("tcpip-pcap-test-" + std::to_string(::getpid()));
    fs::remove_all(directory);
    ASSERT(fs::create_directory(directory));

    fs::path path = directory / "capture.pcap";
    {
        std::ofstream existing(path);
        existing << "old capture";
    }
    fs::permissions(path, fs::perms::all);
    int fd = pcap_open_private_file(path.c_str());
    ASSERT(fd >= 0);
    ASSERT(::close(fd) == 0);
    ASSERT(fs::file_size(path) == 0);
    ASSERT((fs::status(path).permissions() & fs::perms::all) == (fs::perms::owner_read | fs::perms::owner_write));

    fs::path target = directory / "target.pcap";
    fs::path link = directory / "linked.pcap";
    {
        std::ofstream target_file(target);
        target_file << "keep";
    }
    fs::create_symlink(target, link);
    ASSERT(pcap_open_private_file(link.c_str()) == -1);
    ASSERT(fs::file_size(target) == 4);

    fs::path fifo = directory / "blocked.pcap";
    ASSERT(::mkfifo(fifo.c_str(), S_IRUSR | S_IWUSR) == 0);
    ASSERT(pcap_open_private_file(fifo.c_str()) == -1);
    ASSERT(pcap_open_private_file("/dev/null") == -1);

    fs::remove_all(directory);
}
#endif

int main() {
    test_socket_address_to_ip_addr();
    test_ip_addr_to_socket_address();
#ifndef _WIN32
    test_private_pcap_file();
#endif
}
