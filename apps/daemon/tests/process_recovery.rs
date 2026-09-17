#![cfg(windows)]
use cxweb_platform::{loopback, state::protected_directory};
use cxweb_runtime::{
    config_journal::ConfigJournal,
    control_protocol::{Command, Outcome, Reply, Request, exchange},
    lifecycle::DisconnectState,
};
use std::os::windows::process::CommandExt;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command as ProcessCommand, Stdio},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Process(Child);
impl Process {
    fn start(journal: &Path, config: &Path) -> Self {
        Self(
            ProcessCommand::new(env!("CARGO_BIN_EXE_cxweb-daemon"))
                .arg("--journal")
                .arg(journal)
                .arg("--config")
                .arg(config)
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

async fn status(process: &mut Process, installation: &str) -> Reply {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            assert!(
                process.0.try_wait().unwrap().is_none(),
                "runtime exited before private readiness"
            );
            if let Ok(reply) = exchange(
                installation,
                &Request {
                    version: 1,
                    command: Command::Status {},
                },
            )
            .await
            {
                return reply;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}

async fn owned_request(port: u16, capability: &str) -> String {
    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let body = r#"{"model":"webbridge/recovered"}"#;
    let request = format!(
        "POST /wb/{capability}/v1/responses HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(3), socket.read_to_string(&mut response))
        .await
        .unwrap()
        .unwrap();
    response
}

#[tokio::test]
async fn executable_recovers_exact_route_and_catalog_ownership_across_processes() {
    // This subsystem flag prevents an unwanted console when the desktop starts
    // the executable. The fixture never launches a browser or contacts OpenAI.
    let binary = std::fs::read(env!("CARGO_BIN_EXE_cxweb-daemon")).unwrap();
    let pe = u32::from_le_bytes(binary[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(
        u16::from_le_bytes(binary[pe + 92..pe + 94].try_into().unwrap()),
        2
    );
    let fixture =
        Fixture(std::env::temp_dir().join(format!("cxweb-daemon-{:032x}", rand::random::<u128>())));
    protected_directory(&fixture.0).unwrap();
    let directory = fixture.0.join("journal");
    let config = fixture.0.join("config.toml");
    std::fs::write(&config, "model = 'native-safe'\n# preserve this comment\n").unwrap();
    let reserved = loopback::bind(0).unwrap();
    let port = reserved.local_addr().unwrap().port();
    let capability = "c".repeat(43);
    let mut journal = ConfigJournal::prepare(&directory, &config, port, &capability).unwrap();
    journal
        .record_catalog(
            vec!["webbridge/recovered".into()],
            vec!["native-safe".into()],
        )
        .unwrap();
    let installation = journal.installation_id().to_owned();
    journal.apply().unwrap();
    let selected = std::fs::read_to_string(&config)
        .unwrap()
        .replace("native-safe", "webbridge/recovered");
    std::fs::write(&config, &selected).unwrap();
    drop(journal);
    drop(reserved);

    let mut first = Process::start(&directory, &config);
    let Reply::Status {
        instance: first_instance,
        state: DisconnectState::Idle,
        ..
    } = status(&mut first, &installation).await
    else {
        panic!("unexpected recovered status")
    };
    assert!(ConfigJournal::reopen(&directory, &config).is_err());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), selected);
    assert!(
        owned_request(port, &capability)
            .await
            .contains("E_WEB_DISCONNECTED")
    );
    let mut competing = Process::start(&directory, &config);
    let exit = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(exit) = competing.0.try_wait().unwrap() {
                break exit;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(exit.code(), Some(3));
    drop(competing);
    let operation = "d".repeat(32);
    exchange(
        &installation,
        &Request {
            version: 1,
            command: Command::Disconnect {
                instance: first_instance.clone(),
                operation: operation.clone(),
            },
        },
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let reply = exchange(
                &installation,
                &Request {
                    version: 1,
                    command: Command::Operation {
                        instance: first_instance.clone(),
                        operation: operation.clone(),
                    },
                },
            )
            .await
            .unwrap();
            match reply {
                Reply::Operation {
                    outcome:
                        Outcome::Completed {
                            result: DisconnectState::PendingRestart,
                        },
                    ..
                } => break,
                Reply::Operation {
                    outcome: Outcome::Running {},
                    ..
                } => tokio::time::sleep(Duration::from_millis(10)).await,
                other => panic!("unexpected disconnect {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    let restored = std::fs::read_to_string(&config).unwrap();
    assert!(restored.contains("native-safe"));
    assert!(restored.contains("# preserve this comment"));
    assert!(!restored.contains("openai_base_url"));
    drop(first); // Simulated crash of this exact, test-owned child handle.
    let mut second = Process::start(&directory, &config);
    let Reply::Status {
        instance: second_instance,
        state: DisconnectState::PendingRestart,
        ..
    } = status(&mut second, &installation).await
    else {
        panic!("missing restored state")
    };
    assert_ne!(first_instance, second_instance);
    assert!(
        owned_request(port, &capability)
            .await
            .contains("E_WEB_DISCONNECTED")
    );
    assert_eq!(std::fs::read_to_string(&config).unwrap(), restored);
    let stale = exchange(
        &installation,
        &Request {
            version: 1,
            command: Command::Disconnect {
                instance: first_instance,
                operation,
            },
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        stale,
        Reply::Error {
            code: cxweb_runtime::control_protocol::ErrorCode::Instance
        }
    ));
}
