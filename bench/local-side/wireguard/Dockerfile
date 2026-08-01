# syntax=docker/dockerfile:1
FROM bench-ls

ARG WG_QUICK_URL="https://raw.githubusercontent.com/WireGuard/wireguard-tools/71799a8f6d1450b63071a21cad6ed434b348d3d5/src/wg-quick/linux.bash"

RUN apt update -y && apt install -y iptables wireguard

ADD entrypoint.sh \
    wg-quick.patch \
    /bench/
ADD --checksum=sha256:5b2aa96140b9ad8c17c25647fd0d076fb0dc67b3631ee0912c5440cdf43d1f3e \
    "$WG_QUICK_URL" /bench/wg-quick.sh

WORKDIR /bench/
RUN chmod +x wg-quick.sh && \
    patch wg-quick.sh wg-quick.patch

ENTRYPOINT ["./entrypoint.sh"]
CMD ["endpoint_ip", "route"]
