//! Attach desktop windows to installed hosts without starting or replacing them.
use crate::{
    config_journal::ConfigJournal,
    control_protocol::{self, Command, ErrorCode, Outcome, Reply, Request},
};
use cxweb_domain::health::Health;
use cxweb_platform::{state::StatePaths, target_path::TargetPathGuard};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Serialize)]
pub struct Target {
    pub installation: String,
    pub home: PathBuf,
}
#[derive(Debug, Serialize)]
pub struct Inventory {
    pub targets: Vec<Target>,
    pub diagnostics: Vec<&'static str>,
}
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub instance: String,
    pub health: Health,
    pub reasoning: Option<Vec<control_protocol::ReasoningFamily>>,
}

fn inventory(root: &Path) -> Result<Inventory, &'static str> {
    let _guard = TargetPathGuard::capture(root, true).map_err(|_| "E_INSTALLED_STATE")?;
    let entries = std::fs::read_dir(root).map_err(|_| "E_INSTALLED_STATE")?;
    let mut result = Inventory {
        targets: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut ids = BTreeSet::new();
    let mut count = 0;
    for entry in entries.take(257) {
        count += 1;
        if count > 256 {
            result.diagnostics.push("E_INSTALLED_LIMIT");
            break;
        }
        let entry = entry.map_err(|_| "E_INSTALLED_STATE")?;
        let name = entry.file_name();
        let Some(suffix) = name
            .to_str()
            .and_then(|name| name.strip_prefix("prepared-"))
        else {
            continue;
        };
        if suffix.len() != 32 || !suffix.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        match ConfigJournal::control_target(&entry.path()) {
            Ok(Some((installation, home))) => {
                if !ids.insert(installation.clone()) {
                    return Err("E_INSTALLED_DUPLICATE");
                }
                result.targets.push(Target { installation, home });
            }
            Ok(None) => (),
            Err(_) => {
                if !result.diagnostics.contains(&"E_INSTALLED_JOURNAL") {
                    result.diagnostics.push("E_INSTALLED_JOURNAL");
                }
            }
        }
    }
    result.targets.sort_by(|a, b| {
        a.home
            .cmp(&b.home)
            .then(a.installation.cmp(&b.installation))
    });
    Ok(result)
}

pub async fn list() -> Result<Inventory, &'static str> {
    tokio::task::spawn_blocking(|| {
        let root = StatePaths::installations().map_err(|_| "E_INSTALLED_STATE")?;
        let mut current = inventory(&root)?;
        // Keep older installed journals discoverable; never silently migrate a
        // live listener, capability or scheduler registration.
        let legacy = StatePaths::legacy_state().map_err(|_| "E_INSTALLED_STATE")?;
        match std::fs::symlink_metadata(&legacy) {
            Ok(_) => {
                let legacy = inventory(&legacy)?;
                current.targets.extend(legacy.targets);
                current.diagnostics.extend(legacy.diagnostics);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("E_INSTALLED_STATE"),
        }
        let mut ids = BTreeSet::new();
        if current
            .targets
            .iter()
            .any(|target| !ids.insert(target.installation.clone()))
        {
            return Err("E_INSTALLED_DUPLICATE");
        }
        current.targets.sort_by(|a, b| {
            a.home
                .cmp(&b.home)
                .then(a.installation.cmp(&b.installation))
        });
        current.diagnostics.sort_unstable();
        current.diagnostics.dedup();
        Ok(current)
    })
    .await
    .map_err(|_| "E_INSTALLED_STATE")?
}

async fn selected(installation: &str) -> Result<(), &'static str> {
    let found = list().await?;
    if !found.diagnostics.is_empty() {
        return Err("E_INSTALLED_JOURNAL");
    }
    if found.targets.iter().any(|t| t.installation == installation) {
        Ok(())
    } else {
        Err("E_INSTALLED_TARGET")
    }
}

async fn read(installation: &str) -> Result<Snapshot, &'static str> {
    let (instance, health) = match control_protocol::exchange(
        installation,
        &Request {
            version: 1,
            command: Command::Health {},
        },
    )
    .await
    {
        Ok(Reply::Health {
            instance, health, ..
        }) => (instance, *health),
        Ok(_) => return Err("E_INSTALLED_PROTOCOL"),
        Err(_) => return Err("E_INSTALLED_UNAVAILABLE"),
    };
    let reasoning = match control_protocol::exchange(
        installation,
        &Request {
            version: 1,
            command: Command::ReasoningStatus {},
        },
    )
    .await
    {
        Ok(Reply::ReasoningStatus {
            instance: current,
            families,
            ..
        }) if current == instance => families,
        Ok(Reply::ReasoningStatus { .. }) => return Err("E_INSTALLED_CHANGED"),
        // Older installed hosts remain readable during a desktop update.
        Ok(Reply::Error {
            code: ErrorCode::Unsupported | ErrorCode::Protocol,
        }) => None,
        Ok(_) => return Err("E_INSTALLED_PROTOCOL"),
        Err(_) => return Err("E_INSTALLED_UNAVAILABLE"),
    };
    Ok(Snapshot {
        instance,
        health,
        reasoning,
    })
}

pub async fn check(installation: &str) -> Result<Snapshot, &'static str> {
    selected(installation).await?;
    read(installation).await
}

pub async fn disconnect(
    installation: &str,
    instance: String,
    allow_active: bool,
) -> Result<Snapshot, &'static str> {
    selected(installation).await?;
    mutate(installation, instance, Action::Remove(allow_active)).await?;
    read(installation).await
}

pub async fn retry_web(installation: &str, instance: String) -> Result<Snapshot, &'static str> {
    selected(installation).await?;
    mutate(installation, instance, Action::RetryWeb).await?;
    read(installation).await
}

pub async fn verify_compaction(
    installation: &str,
    instance: String,
    target: Option<crate::native_probe::CheckpointTarget>,
) -> Result<Snapshot, &'static str> {
    selected(installation).await?;
    mutate(installation, instance, Action::VerifyCompaction(target)).await?;
    read(installation).await
}

pub async fn qualify_reasoning(
    installation: &str,
    instance: String,
) -> Result<Snapshot, &'static str> {
    selected(installation).await?;
    mutate(installation, instance, Action::QualifyReasoning).await?;
    read(installation).await
}

pub async fn qualify_protocol(
    installation: &str,
    instance: String,
    target: crate::protocol_qualification::Target,
) -> Result<Snapshot, &'static str> {
    selected(installation).await?;
    mutate(installation, instance, Action::QualifyProtocol(target)).await?;
    read(installation).await
}

#[derive(Clone)]
enum Action {
    QualifyProtocol(crate::protocol_qualification::Target),
    VerifyCompaction(Option<crate::native_probe::CheckpointTarget>),
    QualifyReasoning,
    Remove(bool),
    RetryWeb,
}

#[cfg(test)]
async fn remove(
    installation: &str,
    instance: String,
    allow_active: bool,
) -> Result<(), &'static str> {
    mutate(installation, instance, Action::Remove(allow_active)).await
}

async fn mutate(installation: &str, instance: String, action: Action) -> Result<(), &'static str> {
    let operation = format!("{:032x}", rand::random::<u128>());
    let command = match action.clone() {
        Action::QualifyProtocol(target) => Command::QualifyProtocol {
            instance: instance.clone(),
            operation: operation.clone(),
            target,
        },
        Action::VerifyCompaction(target) => Command::VerifyCompaction {
            instance: instance.clone(),
            operation: operation.clone(),
            target,
        },
        Action::QualifyReasoning => Command::QualifyReasoning {
            instance: instance.clone(),
            operation: operation.clone(),
        },
        Action::RetryWeb => Command::RetryWeb {
            instance: instance.clone(),
            operation: operation.clone(),
        },
        Action::Remove(true) => Command::Disconnect {
            instance: instance.clone(),
            operation: operation.clone(),
        },
        Action::Remove(false) => Command::DisconnectWhenIdle {
            instance: instance.clone(),
            operation: operation.clone(),
        },
    };
    let mut reply = control_protocol::exchange(
        installation,
        &Request {
            version: 1,
            command,
        },
    )
    .await;
    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(
            if matches!(
                action,
                Action::QualifyReasoning | Action::QualifyProtocol(_) | Action::VerifyCompaction(_)
            ) {
                2700
            } else if matches!(action, Action::RetryWeb) {
                90
            } else {
                45
            },
        );
    loop {
        match reply {
            Ok(Reply::Operation {
                outcome: Outcome::ReasoningFailed { code },
                ..
            }) => return Err(code.code()),
            Ok(Reply::Operation {
                outcome: Outcome::Completed { .. },
                ..
            }) => return Ok(()),
            Ok(Reply::Operation {
                outcome: Outcome::ActiveWork {},
                ..
            }) => return Err("E_WEB_ACTIVE"),
            Ok(Reply::Operation {
                outcome: Outcome::Failed {},
                ..
            }) => {
                return Err(if matches!(action, Action::QualifyProtocol(_)) {
                    "E_INSTALLED_QUALITY"
                } else if matches!(action, Action::VerifyCompaction(_)) {
                    "E_INSTALLED_COMPACTION"
                } else if matches!(action, Action::RetryWeb) {
                    "E_INSTALLED_RECOVERY"
                } else {
                    "E_INSTALLED_REMOVE"
                });
            }
            Ok(Reply::Operation {
                outcome: Outcome::Running {},
                ..
            })
            | Err(_) => (),
            Ok(Reply::Error {
                code: ErrorCode::Instance,
            }) => return Err("E_INSTALLED_CHANGED"),
            Ok(Reply::Error {
                code: ErrorCode::Busy,
            }) => return Err("E_CONTROL_BUSY"),
            Ok(Reply::Error {
                code: ErrorCode::UnknownOperation,
            }) => return Err("E_INSTALLED_UNCONFIRMED"),
            _ => return Err("E_INSTALLED_PROTOCOL"),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("E_INSTALLED_TIMEOUT");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        // After an ambiguous acknowledgement, only inspect this receipt. Never
        // submit another mutation or attach to a replacement runtime instance.
        reply = control_protocol::exchange(
            installation,
            &Request {
                version: 1,
                command: Command::Operation {
                    instance: instance.clone(),
                    operation: operation.clone(),
                },
            },
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cxweb_platform::{loopback, state::protected_directory};
    #[tokio::test]
    async fn attachment_reads_applied_journals_while_owned_without_exporting_config() {
        let root =
            std::env::temp_dir().join(format!("cxweb-attachment-{:032x}", rand::random::<u128>()));
        protected_directory(&root).unwrap();
        let directory = root.join(format!("prepared-{:032x}", rand::random::<u128>()));
        let config = root.join("config.toml");
        std::fs::write(&config, "# PRIVATE_CONFIG_SENTINEL\n").unwrap();
        let listener = loopback::bind(0).unwrap();
        let mut journal = ConfigJournal::prepare(
            &directory,
            &config,
            listener.local_addr().unwrap().port(),
            &"a".repeat(43),
        )
        .unwrap();
        journal.record_catalog(vec![], vec![]).unwrap();
        assert!(inventory(&root).unwrap().targets.is_empty());
        journal.apply().unwrap();
        let discovered = inventory(&root).unwrap();
        assert_eq!(discovered.targets.len(), 1);
        assert_eq!(
            discovered.targets[0].installation,
            journal.installation_id()
        );
        let json = serde_json::to_string(&discovered).unwrap();
        for secret in ["PRIVATE_CONFIG_SENTINEL", &"a".repeat(43), "127.0.0.1"] {
            assert!(!json.contains(secret));
        }
        let id = journal.installation_id().to_owned();
        drop(journal);
        drop(listener);
        let host = crate::host::Host::recover(&directory, &config).unwrap();
        let server = tokio::spawn(host.serve());
        let initial = read(&id).await.unwrap();
        assert_eq!(
            mutate(&id, initial.instance.clone(), Action::RetryWeb).await,
            Err("E_INSTALLED_RECOVERY")
        );
        assert_eq!(read(&id).await.unwrap().instance, initial.instance);
        assert_eq!(
            remove(&id, "f".repeat(32), false).await,
            Err("E_INSTALLED_CHANGED")
        );
        assert!(
            std::fs::read_to_string(&config)
                .unwrap()
                .contains("openai_base_url")
        );
        remove(&id, initial.instance, false).await.unwrap();
        assert_eq!(
            read(&id).await.unwrap().health.overall,
            cxweb_domain::health::Overall::RemovalPendingRestart
        );
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            "# PRIVATE_CONFIG_SENTINEL\n"
        );
        server.abort();
        let _ = server.await;
        assert_eq!(inventory(&root).unwrap().targets.len(), 1);
        assert!(read(&id).await.is_err());
        let duplicate = root.join(format!("prepared-{:032x}", rand::random::<u128>()));
        protected_directory(&duplicate).unwrap();
        std::fs::copy(
            directory.join("integration.json"),
            duplicate.join("integration.json"),
        )
        .unwrap();
        assert_eq!(inventory(&root).unwrap_err(), "E_INSTALLED_DUPLICATE");
        std::fs::write(duplicate.join("integration.json"), b"invalid journal").unwrap();
        assert_eq!(
            inventory(&root).unwrap().diagnostics,
            ["E_INSTALLED_JOURNAL"]
        );
        // Only this fresh, independently named fixture root is removed.
        std::fs::remove_dir_all(root).unwrap();
    }
}
