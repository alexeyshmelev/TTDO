use crate::http_codec::HttpCodec;
use crate::shutdown::Shutdown;
use crate::{log_id, log_utils};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) async fn listen(
    shutdown: Arc<Mutex<Shutdown>>,
    mut codec: Box<dyn HttpCodec>,
    timeout: Duration,
    log_id: log_utils::IdChain<u64>,
) {
    let (mut shutdown_notification, _shutdown_completion) = {
        let shutdown = shutdown.lock().unwrap();
        (shutdown.notification_handler(), shutdown.completion_guard())
    };

    let listen_task = async {
        match codec.listen().await {
            Ok(Some(x)) => {
                log_id!(
                    trace,
                    log_id,
                    "Received request: {:?}",
                    crate::net_utils::scrub_request(x.request().request())
                );
                if let Err(e) = x.split().1.send_ok_response(true) {
                    log_id!(debug, log_id, "Failed to send ping response: {}", e);
                }
            }
            Ok(None) => log_id!(debug, log_id, "Connection closed before any request"),
            Err(e) => log_id!(debug, log_id, "Session error: {}", e),
        }
    };

    tokio::select! {
        x = shutdown_notification.wait() => {
            match x {
                Ok(_) => (),
                Err(e) => log_id!(debug, log_id, "Shutdown notification failure: {}", e),
            }
        },
        _ = listen_task => (),
        _ = tokio::time::sleep(timeout) => log_id!(debug, log_id, "Session timed out"),
    }

    if let Err(e) = codec.graceful_shutdown().await {
        log_id!(debug, log_id, "Failed to shut down session: {}", e);
    }
}
