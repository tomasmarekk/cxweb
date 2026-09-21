use std::process::Stdio;
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn installed_runtime_serves_mcp_stdio_without_starting_a_browser_or_host() {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_cxweb-daemon"));
    command
        .arg("--web-tools")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n").await.unwrap();
    drop(input);
    let output = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert!(text.contains("\"protocolVersion\":\"2024-11-05\""));
    assert!(text.contains("\"name\":\"search\""));
}
