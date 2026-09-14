//! Carrying the service's answers onto the GTK main loop.
//!
//! `ServiceClient` completes requests on its own threads, because it takes no toolkit dependency,
//! and GTK widgets may only be touched on the main loop. This is the one place the two meet: each
//! request becomes a future that resolves with its answer and is awaited on the main context, so
//! no widget is ever touched from a client thread.

use std::future::Future;

use goosic_protocol::{RequestPayload, ResponseEnvelope};
use goosic_shell_support::{ServiceClient, TransportError};

/// Sends `command` and returns a future for its answer. Await it with
/// `glib::spawn_future_local`, or `MainContext::block_on` in a test.
pub fn request(
    client: &ServiceClient,
    command: &str,
    payload: RequestPayload,
) -> impl Future<Output = Result<ResponseEnvelope, TransportError>> {
    // One slot is enough: the client calls a completion exactly once.
    let (answer, answered) = async_channel::bounded(1);
    client.send(command, payload, move |result| {
        // Nobody may be waiting any more — the window that asked could be gone. That is not an
        // error worth reporting from a client thread.
        let _ = answer.send_blocking(result);
    });
    async move {
        answered.recv().await.unwrap_or_else(|_| {
            Err(TransportError::Unavailable(
                "the answer was dropped before it arrived".to_owned(),
            ))
        })
    }
}
