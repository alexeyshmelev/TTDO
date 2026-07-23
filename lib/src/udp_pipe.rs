use crate::{datagram_pipe, downstream, forwarder, log_id, log_utils, net_utils, pipe};
use async_trait::async_trait;
use futures::future;
use futures::future::Either;
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

pub(crate) struct DuplexPipe<F: Send + Sync> {
    left_pipe: LeftPipe<F>,
    right_pipe: RightPipe<F>,
    timeout: Duration,
}

/// Forwards UDP packets from a client to a target host
struct LeftPipe<F: Send + Sync> {
    source: Box<dyn datagram_pipe::Source<Output = downstream::UdpDatagram>>,
    sink: Box<dyn datagram_pipe::Sink<Input = downstream::UdpDatagram>>,
    shared: Arc<UdpPipeShared<F>>,
    direction: pipe::SimplexDirection,
    next_connection_id: std::ops::RangeFrom<u64>,
}

/// Forwards UDP packets from a target host to a client
struct RightPipe<F: Send + Sync> {
    source: Box<dyn datagram_pipe::Source<Output = forwarder::UdpDatagramReadStatus>>,
    sink: Box<dyn datagram_pipe::Sink<Input = forwarder::UdpDatagram>>,
    shared: Arc<UdpPipeShared<F>>,
    direction: pipe::SimplexDirection,
}

struct UdpPipeShared<F: Send + Sync> {
    udp_connections: Mutex<HashMap<forwarder::UdpDatagramMeta, UdpConnection>>,
    forwarder_shared: Arc<dyn forwarder::UdpDatagramPipeShared>,
    update_metrics: F,
}

struct UdpConnection {
    last_activity: Instant,
    plain_dns_info: Option<PlainDnsInfo>,
    log_id: log_utils::IdChain<u64>,
}

struct PlainDnsInfo {
    pending_queries: usize,
}

#[derive(Eq, PartialEq)]
enum UdpConnectionStatus {
    Continue,
    Done,
}

impl<F: Fn(pipe::SimplexDirection, usize) + Send + Sync> LeftPipe<F> {
    async fn exchange(&mut self) -> io::Result<()> {
        loop {
            let datagram = self.source.read().await?;
            log_id!(trace, self.source.id(), "--> Datagram: {:?}", datagram);

            if let Err(e) = self.on_udp_packet(&datagram.meta).await {
                log_id!(
                    debug,
                    self.source.id(),
                    "--> Dropping UDP packet due to error: datagram={:?}, error={}",
                    datagram,
                    e
                );
                continue;
            }

            let datagram_len = datagram.payload.len();
            match self.sink.write(datagram).await? {
                datagram_pipe::SendStatus::Sent => {
                    (self.shared.update_metrics)(self.direction, datagram_len);
                }
                datagram_pipe::SendStatus::Dropped => {
                    log_id!(trace, self.source.id(), "--> Datagram dropped")
                }
            }
        }
    }

    async fn on_udp_packet(&mut self, meta: &downstream::UdpDatagramMeta) -> io::Result<()> {
        if let Some(conn) = self
            .shared
            .udp_connections
            .lock()
            .unwrap()
            .get_mut(&forwarder::UdpDatagramMeta::from(meta))
        {
            conn.register_outgoing_packet();
            return Ok(());
        }

        self.shared
            .forwarder_shared
            .on_new_udp_connection(meta)
            .await?;

        let is_plain_dns = meta.destination.port() == net_utils::PLAIN_DNS_PORT_NUMBER;
        let mut connection = UdpConnection {
            last_activity: Instant::now(),
            plain_dns_info: is_plain_dns.then_some(PlainDnsInfo { pending_queries: 0 }),
            log_id: self.source.id().extended(log_utils::IdItem::new(
                log_utils::CONNECTION_ID_FMT,
                self.next_connection_id.next().unwrap(),
            )),
        };
        connection.register_outgoing_packet();
        self.shared
            .udp_connections
            .lock()
            .unwrap()
            .insert(forwarder::UdpDatagramMeta::from(meta), connection);
        Ok(())
    }
}

impl<F: Fn(pipe::SimplexDirection, usize) + Send + Sync> RightPipe<F> {
    async fn exchange(&mut self) -> io::Result<()> {
        loop {
            let datagram = match self.source.read().await? {
                forwarder::UdpDatagramReadStatus::Read(x) => x,
                forwarder::UdpDatagramReadStatus::UdpClose(meta, e) => {
                    if let Some(c) = self.shared.udp_connections.lock().unwrap().remove(&meta) {
                        log_id!(
                            debug,
                            c.log_id,
                            "Connection closed: meta={:?} error={}",
                            meta,
                            e
                        );
                    }
                    continue;
                }
            };
            log_id!(trace, self.source.id(), "<-- Datagram: {:?}", datagram);

            let meta = datagram.meta;
            let datagram_len = datagram.payload.len();
            match self.sink.write(datagram).await? {
                datagram_pipe::SendStatus::Sent => {
                    (self.shared.update_metrics)(self.direction, datagram_len);
                }
                datagram_pipe::SendStatus::Dropped => {
                    log_id!(trace, self.source.id(), "<-- Datagram dropped")
                }
            }

            let reversed = meta.reversed();
            let x = self.on_udp_packet(&reversed);
            match x {
                UdpConnectionStatus::Continue => (),
                UdpConnectionStatus::Done => {
                    if let Some(c) = self
                        .shared
                        .udp_connections
                        .lock()
                        .unwrap()
                        .remove(&reversed)
                    {
                        log_id!(debug, c.log_id, "All UDP queries are completed");
                    }
                    self.shared.forwarder_shared.on_connection_closed(&meta);
                }
            }
        }
    }

    fn on_udp_packet(&mut self, meta: &forwarder::UdpDatagramMeta) -> UdpConnectionStatus {
        match self.shared.udp_connections.lock().unwrap().get_mut(meta) {
            None => UdpConnectionStatus::Continue,
            Some(conn) => conn.register_incoming_packet(),
        }
    }
}

impl UdpConnection {
    fn register_outgoing_packet(&mut self) {
        self.last_activity = Instant::now();
        if let Some(info) = self.plain_dns_info.as_mut() {
            info.pending_queries += 1;
        }
    }

    fn register_incoming_packet(&mut self) -> UdpConnectionStatus {
        self.last_activity = Instant::now();
        self.plain_dns_info
            .as_mut()
            .map_or(UdpConnectionStatus::Continue, |info| {
                info.pending_queries = info.pending_queries.saturating_sub(1);
                if info.pending_queries == 0 {
                    UdpConnectionStatus::Done
                } else {
                    UdpConnectionStatus::Continue
                }
            })
    }
}

impl<F: Fn(pipe::SimplexDirection, usize) + Send + Sync> DuplexPipe<F> {
    #[allow(clippy::type_complexity)]
    pub fn new(
        (source1, sink1): (
            Box<dyn datagram_pipe::Source<Output = downstream::UdpDatagram>>,
            Box<dyn datagram_pipe::Sink<Input = forwarder::UdpDatagram>>,
        ),
        (shared2, source2, sink2): (
            Arc<dyn forwarder::UdpDatagramPipeShared>,
            Box<dyn datagram_pipe::Source<Output = forwarder::UdpDatagramReadStatus>>,
            Box<dyn datagram_pipe::Sink<Input = downstream::UdpDatagram>>,
        ),
        update_metrics: F,
        timeout: Duration,
    ) -> Self {
        let shared = Arc::new(UdpPipeShared {
            udp_connections: Mutex::new(Default::default()),
            forwarder_shared: shared2,
            update_metrics,
        });

        Self {
            left_pipe: LeftPipe {
                source: source1,
                sink: sink2,
                shared: shared.clone(),
                direction: pipe::SimplexDirection::Outgoing,
                next_connection_id: 0..,
            },
            right_pipe: RightPipe {
                source: source2,
                sink: sink1,
                shared,
                direction: pipe::SimplexDirection::Incoming,
            },
            timeout,
        }
    }

    async fn exchange_once(&mut self) -> io::Result<()> {
        let left = self.left_pipe.exchange();
        futures::pin_mut!(left);
        let right = self.right_pipe.exchange();
        futures::pin_mut!(right);
        match future::try_select(left, right).await {
            Ok(_) => Ok(()),
            Err(Either::Left((e, _))) | Err(Either::Right((e, _))) => Err(e),
        }
    }

    fn on_timer_tick(&mut self) {
        let last_unexpired_timestamp = Instant::now() - self.timeout;

        let mut connections = self.left_pipe.shared.udp_connections.lock().unwrap();
        let expired: Vec<_> = connections
            .iter()
            .filter(|(_, conn)| conn.last_activity < last_unexpired_timestamp)
            .map(|(meta, c)| (*meta, c.log_id.clone()))
            .collect();

        for (meta, id) in expired {
            connections.remove(&meta);
            self.right_pipe
                .shared
                .forwarder_shared
                .on_connection_closed(&meta);
            log_id!(debug, id, "Connection expired: {:?}", meta);
        }
    }
}

#[async_trait]
impl<F: Fn(pipe::SimplexDirection, usize) + Send + Sync> datagram_pipe::DuplexPipe
    for DuplexPipe<F>
{
    async fn exchange(&mut self) -> io::Result<()> {
        loop {
            match tokio::time::timeout(self.timeout / 4, self.exchange_once()).await {
                Ok(x) => return x,
                Err(_) => self.on_timer_tick(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct UnusedSource;

    #[async_trait]
    impl datagram_pipe::Source for UnusedSource {
        type Output = downstream::UdpDatagram;

        fn id(&self) -> log_utils::IdChain<u64> {
            log_utils::IdChain::empty()
        }

        async fn read(&mut self) -> io::Result<Self::Output> {
            unreachable!()
        }
    }

    struct UnusedSink;

    #[async_trait]
    impl datagram_pipe::Sink for UnusedSink {
        type Input = downstream::UdpDatagram;

        async fn write(&mut self, _data: Self::Input) -> io::Result<datagram_pipe::SendStatus> {
            unreachable!()
        }
    }

    struct FailingForwarder {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl forwarder::UdpDatagramPipeShared for FailingForwarder {
        async fn on_new_udp_connection(
            &self,
            _meta: &downstream::UdpDatagramMeta,
        ) -> io::Result<()> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Err(io::Error::other("expected failure"))
        }

        fn on_connection_closed(&self, _meta: &forwarder::UdpDatagramMeta) {}
    }

    fn update_metrics(_direction: pipe::SimplexDirection, _size: usize) {}

    #[tokio::test]
    async fn failed_connection_creation_is_retried_without_stale_state() {
        let forwarder = Arc::new(FailingForwarder {
            calls: AtomicUsize::new(0),
        });
        let shared = Arc::new(UdpPipeShared {
            udp_connections: Mutex::new(HashMap::new()),
            forwarder_shared: forwarder.clone(),
            update_metrics,
        });
        let mut pipe = LeftPipe {
            source: Box::new(UnusedSource),
            sink: Box::new(UnusedSink),
            shared: shared.clone(),
            direction: pipe::SimplexDirection::Outgoing,
            next_connection_id: 0..,
        };
        let meta = downstream::UdpDatagramMeta {
            source: SocketAddr::from((Ipv4Addr::LOCALHOST, 12345)),
            destination: SocketAddr::from((Ipv4Addr::new(192, 0, 2, 1), 53)),
            app_name: None,
        };

        assert!(pipe.on_udp_packet(&meta).await.is_err());
        assert!(pipe.on_udp_packet(&meta).await.is_err());
        assert_eq!(forwarder.calls.load(Ordering::Relaxed), 2);
        assert!(shared.udp_connections.lock().unwrap().is_empty());
    }
}
