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
