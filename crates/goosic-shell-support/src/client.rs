//! A client of `goosic-service` that any shell can link.
//!
//! Several requests may be outstanding at once and each completes on its own. That is not a
//! throughput optimisation; it is what keeps the transport controls working. The service answers
//! catalog and lyrics reads off its main loop, so a browse waiting on a third-party host no longer
//! holds a pause or a seek behind it — but only if the client does not hold them either. A client
//! that wrote one request and then blocked reading until *that* answer arrived would rebuild the
//! queue the service just took apart. Requests are matched by id, so the order answers arrive in
//! does not matter and does not have to.
//!
//! A timeout fails one request and leaves the child running. Teardown is reserved for the failures
//! that genuinely corrupt the stream: an undecodable line, a protocol version mismatch, an
//! oversized frame, or end of file. Those fail every outstanding request with the same error, so
//! nothing is left waiting for a service that is gone.
//!
//! Completions run on the client's own threads, never on the caller's. A shell hands them to its
//! UI thread however its toolkit does that; this crate takes no toolkit dependency, so it cannot
//! do it for them. A completion must not call [`ServiceClient::request`]: that blocks until an
//! answer is delivered, and the thread it would block is the one that delivers answers.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use goosic_protocol::{RequestEnvelope, RequestPayload, ResponseEnvelope, PROTOCOL_VERSION};

use crate::{decode_response, into_result, timeout_for, FrameReader, TransportError};

type Completion = Box<dyn FnOnce(Result<ResponseEnvelope, TransportError>) + Send + 'static>;
type TimeoutPolicy = Arc<dyn Fn(&str) -> Duration + Send + Sync + 'static>;

/// How a [`ServiceClient`] is launched.
pub struct ServiceClientBuilder {
    program: OsString,
    timeouts: TimeoutPolicy,
    id_prefix: String,
}

impl ServiceClientBuilder {
    /// A client for the service at `program`. A bare name is looked up on `PATH`.
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            timeouts: Arc::new(timeout_for),
            id_prefix: "shell".to_owned(),
        }
    }

    /// The service a shell uses when nobody said otherwise: `GOOSIC_SERVICE_PATH` when it is set,
    /// otherwise `goosic-service` from `PATH`.
    pub fn from_environment() -> Self {
        Self::new(
            std::env::var_os("GOOSIC_SERVICE_PATH").unwrap_or_else(|| "goosic-service".into()),
        )
    }

    /// Replaces [`timeout_for`]. This exists for tests, which need deadlines short enough to
    /// exercise without sleeping for the length of a real catalog read.
    pub fn timeouts(mut self, policy: impl Fn(&str) -> Duration + Send + Sync + 'static) -> Self {
        self.timeouts = Arc::new(policy);
        self
    }

    /// What request ids start with, so a transcript shows which shell sent a line.
    pub fn request_id_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.id_prefix = prefix.into();
        self
    }

    /// Starts the service as a private child process and begins reading its answers.
    ///
    /// Stderr is inherited: it is the service's diagnostic channel and belongs in the shell's own
    /// log. Stdout is the protocol and nothing else reads it.
    pub fn spawn(self) -> Result<ServiceClient, TransportError> {
        let mut child = Command::new(&self.program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                TransportError::Unavailable(format!("could not launch goosic-service: {error}"))
            })?;
        let stdin = child.stdin.take().expect("stdin was requested as a pipe");
        let stdout = child.stdout.take().expect("stdout was requested as a pipe");
        let (writer, lines) = mpsc::channel();

        let inner = Arc::new(Inner {
            state: Mutex::new(State {
                next_number: 0,
                pending: HashMap::new(),
                closed: None,
                writer: Some(writer),
                child: Some(child),
            }),
            wake_timer: Condvar::new(),
            timeouts: self.timeouts,
            id_prefix: self.id_prefix,
        });

        let started = start_thread("goosic-service-writer", {
            let inner = inner.clone();
            move || write_loop(&inner, stdin, lines)
        })
        .and_then(|_| {
            start_thread("goosic-service-reader", {
                let inner = inner.clone();
                move || read_loop(&inner, stdout)
            })
        })
        .and_then(|_| {
            start_thread("goosic-service-deadlines", {
                let inner = inner.clone();
                move || deadline_loop(&inner)
            })
        });
        if let Err(error) = started {
            inner.invalidate(error.clone());
            return Err(error);
        }
        Ok(ServiceClient { inner })
    }
}

/// One private conversation with one `goosic-service` child.
///
/// Dropping the client ends the conversation: outstanding requests are answered with
/// [`TransportError::Unavailable`] and the child is stopped, so a closed window cannot leave an
/// orphan service behind.
pub struct ServiceClient {
    inner: Arc<Inner>,
}

impl ServiceClient {
    /// Sends `command` and calls `completion` exactly once with its answer.
    ///
    /// Returns the request id, or `None` when the client is already closed — in which case
    /// `completion` has already been called with the reason.
    pub fn send(
        &self,
        command: &str,
        payload: RequestPayload,
        completion: impl FnOnce(Result<ResponseEnvelope, TransportError>) + Send + 'static,
    ) -> Option<String> {
        let completion: Completion = Box::new(completion);
        let mut state = self.inner.lock();
        if let Some(reason) = &state.closed {
            let error = no_longer_available(reason);
            drop(state);
            completion(Err(error));
            return None;
        }
        state.next_number += 1;
        let request_id = format!("{}-{}", self.inner.id_prefix, state.next_number);
        let line = encode(&request_id, command, payload);
        let deadline = Instant::now() + (self.inner.timeouts)(command);
        state.pending.insert(request_id.clone(), Pending { completion, deadline });
        // The writer thread owns the pipe, so a full pipe buffer can never stall the caller —
        // which, for every shell, is its UI thread.
        let queued = state.writer.as_ref().is_some_and(|writer| writer.send(line).is_ok());
        drop(state);
        self.inner.wake_timer.notify_all();
        if !queued {
            self.inner.fail(
                &request_id,
                TransportError::Unavailable("the service's input is closed".to_owned()),
            );
        }
        Some(request_id)
    }

    /// Sends `command` and waits for its answer. For tests and command-line tools; a shell's UI
    /// thread uses [`send`](Self::send), and a completion must never call this.
    pub fn request(
        &self,
        command: &str,
        payload: RequestPayload,
    ) -> Result<ResponseEnvelope, TransportError> {
        let (answer, answered) = mpsc::channel();
        self.send(command, payload, move |result| {
            let _ = answer.send(result);
        });
        answered.recv().unwrap_or_else(|_| {
            Err(TransportError::Unavailable("the request was abandoned".to_owned()))
        })
    }

    /// Why the conversation ended, or `None` while it is still open.
    pub fn closed_reason(&self) -> Option<TransportError> {
        self.inner.lock().closed.clone()
    }
}

impl Drop for ServiceClient {
    fn drop(&mut self) {
        self.inner.invalidate(TransportError::Unavailable("the client was closed".to_owned()));
    }
}

struct Inner {
    state: Mutex<State>,
    wake_timer: Condvar,
    timeouts: TimeoutPolicy,
    id_prefix: String,
}

struct State {
    next_number: u64,
    pending: HashMap<String, Pending>,
    closed: Option<TransportError>,
    writer: Option<Sender<Vec<u8>>>,
    child: Option<Child>,
}

struct Pending {
    completion: Completion,
    deadline: Instant,
}

impl Inner {
    /// Poisoning is ignored on purpose. A completion panicking on another thread says nothing
    /// about the table, which is only ever changed in single statements under this lock.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn deliver(&self, response: ResponseEnvelope) {
        // A response with no waiting request belongs to a caller that already gave up. Dropping it
        // is correct; it is not evidence the stream is corrupt, and treating it as such is how a
        // single timeout used to take the service down.
        let pending = self.lock().pending.remove(&response.request_id);
        if let Some(pending) = pending {
            (pending.completion)(into_result(response));
        }
    }

    fn fail(&self, request_id: &str, error: TransportError) {
        let pending = self.lock().pending.remove(request_id);
        if let Some(pending) = pending {
            (pending.completion)(Err(error));
        }
    }

    fn is_closed(&self) -> bool {
        self.lock().closed.is_some()
    }

    /// The stream cannot be trusted any further: close the input, stop the child so a blocked
    /// reader cannot race a later request or leave an orphan process, and give every outstanding
    /// request the same answer rather than leaving it waiting for a service that is gone.
    fn invalidate(&self, error: TransportError) {
        let (outstanding, child) = {
            let mut state = self.lock();
            if state.closed.is_some() {
                return;
            }
            state.closed = Some(error.clone());
            // Dropping the sender ends the writer thread, which drops the pipe.
            state.writer = None;
            (std::mem::take(&mut state.pending), state.child.take())
        };
        self.wake_timer.notify_all();
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
        for (_, pending) in outstanding {
            (pending.completion)(Err(error.clone()));
        }
    }
}

fn no_longer_available(reason: &TransportError) -> TransportError {
    TransportError::Unavailable(format!("goosic-service is no longer available ({reason})"))
}

fn encode(request_id: &str, command: &str, payload: RequestPayload) -> Vec<u8> {
    let request = RequestEnvelope {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        request_id: request_id.to_owned(),
        command: command.to_owned(),
        payload,
    };
    // serde_json fails to serialise only maps with non-string keys or a failing custom
    // `Serialize`, and the protocol types have neither.
    let mut line = serde_json::to_vec(&request).expect("a protocol request always encodes");
    line.push(b'\n');
    line
}

fn start_thread(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), TransportError> {
    thread::Builder::new().name(name.to_owned()).spawn(body).map(|_| ()).map_err(|error| {
        TransportError::Unavailable(format!("could not start the {name} thread: {error}"))
    })
}

fn write_loop(inner: &Inner, mut stdin: ChildStdin, lines: Receiver<Vec<u8>>) {
    for line in lines {
        if stdin.write_all(&line).and_then(|_| stdin.flush()).is_err() {
            inner.invalidate(TransportError::Unavailable(
                "could not write to goosic-service".to_owned(),
            ));
            return;
        }
    }
    // The sender is gone, so the conversation is over. Dropping `stdin` here closes the service's
    // input, which is its own signal to finish and exit.
}

fn read_loop(inner: &Inner, mut stdout: ChildStdout) {
    let mut frames = FrameReader::default();
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        let read = match stdout.read(&mut chunk) {
            Ok(0) => {
                inner.invalidate(TransportError::EndOfFile);
                return;
            }
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => {
                inner.invalidate(TransportError::EndOfFile);
                return;
            }
        };
        frames.push(&chunk[..read]);
        // One read can carry several responses, so this keeps taking them until only a partial
        // frame is left.
        loop {
            match frames.next_frame().and_then(|frame| frame.map(|f| decode_response(&f)).transpose())
            {
                Ok(Some(response)) => inner.deliver(response),
                Ok(None) => break,
                Err(error) => {
                    inner.invalidate(error);
                    return;
                }
            }
        }
        if inner.is_closed() {
            return;
        }
    }
}

fn deadline_loop(inner: &Inner) {
    let mut state = inner.lock();
    loop {
        if state.closed.is_some() {
            return;
        }
        let now = Instant::now();
        let expired: Vec<String> = state
            .pending
            .iter()
            .filter(|(_, pending)| pending.deadline <= now)
            .map(|(request_id, _)| request_id.clone())
            .collect();
        if !expired.is_empty() {
            let completions: Vec<Completion> = expired
                .iter()
                .filter_map(|request_id| state.pending.remove(request_id))
                .map(|pending| pending.completion)
                .collect();
            drop(state);
            // A request still waiting at its deadline gets an answer of its own. The service keeps
            // running: one read that took too long says nothing about the next command.
            for completion in completions {
                completion(Err(TransportError::TimedOut));
            }
            state = inner.lock();
            continue;
        }
        let next_deadline = state.pending.values().map(|pending| pending.deadline).min();
        state = match next_deadline {
            Some(deadline) => {
                let wait = deadline.saturating_duration_since(now);
                match inner.wake_timer.wait_timeout(state, wait) {
                    Ok((guard, _)) => guard,
                    Err(poisoned) => poisoned.into_inner().0,
                }
            }
            None => inner.wake_timer.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner()),
        };
    }
}

/// These run against a stub that speaks the same NDJSON protocol and sleeps exactly when asked
/// to, because the property worth proving — a slow answer does not delay a fast one — needs a
/// service whose timing the test controls. The real service is slow only when a third-party host
/// is, and a test may not depend on that.
///
/// The cases mirror the Swift shell's `ServiceClientConcurrencyTests` one for one, so the two
/// clients are held to the same behaviour rather than to two readings of it.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// Answers `slow.*` after `delay` seconds and everything else at once, each on its own thread,
    /// echoing the command as `message` so a response delivered to the wrong completion shows up.
    /// A few prefixes misbehave on purpose.
    const STUB: &str = r#"#!/usr/bin/env python3
import json, os, sys, threading, time

DELAY = float(sys.argv[1])
write = threading.Lock()

def emit(text):
    with write:
        sys.stdout.write(text + "\n")
        sys.stdout.flush()

def answer(request):
    command = request["command"]
    if command.startswith("slow."):
        time.sleep(DELAY)
    if command.startswith("corrupt."):
        return emit("this is not a protocol frame")
    if command.startswith("exit."):
        os._exit(0)
    version = "0.4.0" if command.startswith("foreign.") else "0.3.0"
    if command.startswith("refuse."):
        return emit(json.dumps({
            "protocolVersion": version, "requestId": request["requestId"], "ok": False,
            "payload": None, "error": {"code": "ownerConflict", "message": "held"},
        }))
    emit(json.dumps({
        "protocolVersion": version, "requestId": request["requestId"], "ok": True,
        "payload": {"message": command}, "error": None,
    }))

threads = []
for line in sys.stdin:
    line = line.strip()
    if line:
        thread = threading.Thread(target=answer, args=(json.loads(line),))
        thread.start()
        threads.append(thread)
for thread in threads:
    thread.join()
"#;

    struct Stub {
        directory: std::path::PathBuf,
        script: std::path::PathBuf,
    }

    impl Stub {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let directory = std::env::temp_dir().join(format!(
                "goosic-stub-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&directory).unwrap();
            let script = directory.join("stub-service");
            std::fs::write(&script, STUB).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self { directory, script }
        }

        /// A launcher that passes the delay through, since the builder takes a program and no
        /// arguments — exactly as a shell does.
        fn with_delay(&self, delay: f64) -> std::path::PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let launcher = self.directory.join(format!("launch-{delay}"));
            std::fs::write(
                &launcher,
                format!("#!/bin/sh\nexec \"{}\" {delay}\n", self.script.display()),
            )
            .unwrap();
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
            launcher
        }
    }

    impl Drop for Stub {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn message(result: &Result<ResponseEnvelope, TransportError>) -> Option<String> {
        result.as_ref().ok()?.payload.as_ref()?.message.clone()
    }

    /// The failure this exists for: a catalog browse waiting on a third-party host used to hold
    /// every later command behind it, so pause and seek did nothing until it finished.
    #[test]
    fn a_slow_request_does_not_delay_the_commands_behind_it() {
        let stub = Stub::new();
        let client = ServiceClientBuilder::new(stub.with_delay(2.0)).spawn().unwrap();
        let (slow_answer, slow_answered) = mpsc::channel();
        let started = Instant::now();
        client.send("slow.browse", RequestPayload::default(), move |result| {
            let _ = slow_answer.send(result);
        });
        let fast = client.request("playback.sample", RequestPayload::default());
        let elapsed = started.elapsed();
        assert_eq!(message(&fast).as_deref(), Some("playback.sample"));
        assert!(
            elapsed < Duration::from_secs(1),
            "the transport command waited {elapsed:?} behind a 2s read instead of being answered"
        );
        let slow = slow_answered.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(message(&slow).as_deref(), Some("slow.browse"));
    }

    /// Answers are matched by request id, which is what makes the order they arrive in irrelevant.
    #[test]
    fn every_outstanding_request_gets_its_own_answer() {
        let stub = Stub::new();
        let client = ServiceClientBuilder::new(stub.with_delay(0.4)).spawn().unwrap();
        let commands = ["slow.a", "playback.claim", "slow.b", "state.get", "slow.c"];
        let (answer, answers) = mpsc::channel();
        for command in commands {
            let answer = answer.clone();
            client.send(command, RequestPayload::default(), move |result| {
                let _ = answer.send((command, result));
            });
        }
        for _ in commands {
            let (command, result) = answers.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(message(&result).as_deref(), Some(command), "{command} got another answer");
        }
    }

    /// A request that outlives its deadline is failed on its own. It used to take the child
    /// process with it, which meant one unanswered catalog read stopped playback. Its answer still
    /// arrives later, for an id nobody is waiting on, and has to be dropped rather than delivered
    /// twice or treated as corruption.
    #[test]
    fn a_timed_out_request_does_not_take_the_service_down() {
        let stub = Stub::new();
        let client = ServiceClientBuilder::new(stub.with_delay(1.0))
            .timeouts(|command| {
                if command.starts_with("slow.") {
                    Duration::from_millis(200)
                } else {
                    Duration::from_secs(5)
                }
            })
            .spawn()
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let (answer, answered) = mpsc::channel();
        {
            let calls = calls.clone();
            client.send("slow.stalled", RequestPayload::default(), move |result| {
                calls.fetch_add(1, Ordering::SeqCst);
                let _ = answer.send(result);
            });
        }
        let result = answered.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(result.unwrap_err(), TransportError::TimedOut);
        assert_eq!(client.closed_reason(), None, "a timeout closed the client");

        // Let the late answer arrive, then prove the client ignored it and still works.
        thread::sleep(Duration::from_millis(1200));
        let later = client.request("state.get", RequestPayload::default());
        assert_eq!(message(&later).as_deref(), Some("state.get"));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the timed-out completion ran twice");
    }

    #[test]
    fn a_refusal_is_an_answer_and_the_conversation_continues() {
        let stub = Stub::new();
        let client = ServiceClientBuilder::new(stub.with_delay(0.0)).spawn().unwrap();
        let refused = client.request("refuse.claim", RequestPayload::default()).unwrap_err();
        assert_eq!(
            refused,
            TransportError::Remote { code: "ownerConflict".into(), message: "held".into() }
        );
        assert_eq!(client.closed_reason(), None);
        assert!(client.request("state.get", RequestPayload::default()).is_ok());
    }

    /// After an undecodable line there is no way to know where the next frame begins, so every
    /// request still waiting — including one that has nothing to do with the bad line — gets the
    /// same answer, and the client refuses new work instead of queueing it for a dead stream.
    #[test]
    fn an_undecodable_line_fails_everything_outstanding_and_closes_the_client() {
        let stub = Stub::new();
        let client = ServiceClientBuilder::new(stub.with_delay(2.0)).spawn().unwrap();
        let (answer, answered) = mpsc::channel();
        client.send("slow.innocent", RequestPayload::default(), move |result| {
            let _ = answer.send(result);
        });
        let corrupt = client.request("corrupt.frame", RequestPayload::default());
        assert_eq!(corrupt.unwrap_err(), TransportError::InvalidResponse);
        let innocent = answered.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(innocent.unwrap_err(), TransportError::InvalidResponse);
        assert_eq!(client.closed_reason(), Some(TransportError::InvalidResponse));
        let refused = client.request("state.get", RequestPayload::default()).unwrap_err();
        assert!(matches!(refused, TransportError::Unavailable(_)));
    }

    #[test]
    fn a_service_speaking_another_protocol_version_closes_the_client() {
        let stub = Stub::new();
        let client = ServiceClientBuilder::new(stub.with_delay(0.0)).spawn().unwrap();
        let error = client.request("foreign.hello", RequestPayload::default()).unwrap_err();
        assert!(matches!(error, TransportError::ProtocolVersionMismatch { .. }));
        assert!(client.closed_reason().is_some());
    }

    #[test]
    fn the_service_exiting_answers_what_was_outstanding() {
        let stub = Stub::new();
        let client = ServiceClientBuilder::new(stub.with_delay(5.0)).spawn().unwrap();
        let (answer, answered) = mpsc::channel();
        client.send("slow.never", RequestPayload::default(), move |result| {
            let _ = answer.send(result);
        });
        let exit = client.request("exit.now", RequestPayload::default());
        assert_eq!(exit.unwrap_err(), TransportError::EndOfFile);
        let never = answered.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(never.unwrap_err(), TransportError::EndOfFile);
    }

    #[test]
    fn dropping_the_client_answers_what_was_outstanding() {
        let stub = Stub::new();
        let client = ServiceClientBuilder::new(stub.with_delay(5.0)).spawn().unwrap();
        let (answer, answered) = mpsc::channel();
        client.send("slow.abandoned", RequestPayload::default(), move |result| {
            let _ = answer.send(result);
        });
        let started = Instant::now();
        drop(client);
        let abandoned = answered.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(abandoned, Err(TransportError::Unavailable(_))));
        assert!(started.elapsed() < Duration::from_secs(2), "closing waited for the service");
    }

    #[test]
    fn a_service_that_cannot_be_launched_says_so() {
        let result = ServiceClientBuilder::new("/nonexistent/goosic-service").spawn();
        assert!(matches!(result, Err(TransportError::Unavailable(_))));
    }
}
