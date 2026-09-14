use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn binary_processes_one_request_per_line() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_goosic-service"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("service binary should start");
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "{}", "x".repeat(64 * 1024 + 1)).unwrap();
    writeln!(
        input,
        r#"{{"protocolVersion":"0.3.0","requestId":"1","command":"hello","payload":{{}}}}"#
    )
    .unwrap();
    writeln!(
        input,
        r#"{{"protocolVersion":"0.3.0","requestId":"2","command":"state.get","payload":{{}}}}"#
    )
    .unwrap();
    drop(input);

    let stdout = child.stdout.take().unwrap();
    let lines: Vec<String> = BufReader::new(stdout).lines().map(Result::unwrap).collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains(r#""code":"frameTooLarge""#));
    assert!(lines[1].contains(r#""requestId":"1""#));
    assert!(lines[2].contains(r#""requestId":"2""#));
    assert!(child.wait().unwrap().success());
}

/// Catalog and lyrics reads are answered off the main loop, so their responses no longer have to
/// arrive in the order the requests were sent. This asserts what a client may actually rely on:
/// every request is answered exactly once and is identified by its request id. An empty query is
/// rejected before any network access, which keeps the test offline and deterministic while still
/// exercising the worker path end to end.
#[test]
fn concurrent_reads_are_answered_by_request_id_in_any_order() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_goosic-service"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("service binary should start");
    let mut input = child.stdin.take().unwrap();
    for (id, command, payload) in [
        ("read-1", "catalog.search", r#"{"query":""}"#),
        ("serial-1", "state.get", "{}"),
        ("read-2", "lyrics.get", "{}"),
        ("serial-2", "hello", "{}"),
    ] {
        writeln!(
            input,
            r#"{{"protocolVersion":"0.3.0","requestId":"{id}","command":"{command}","payload":{payload}}}"#
        )
        .unwrap();
    }
    drop(input);

    let stdout = child.stdout.take().unwrap();
    let lines: Vec<String> = BufReader::new(stdout).lines().map(Result::unwrap).collect();
    assert_eq!(lines.len(), 4, "every request is answered exactly once");
    for id in ["read-1", "serial-1", "read-2", "serial-2"] {
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains(&format!(r#""requestId":"{id}""#)))
                .count(),
            1,
            "exactly one response carries {id}: {lines:?}"
        );
    }
    assert!(child.wait().unwrap().success());
}
