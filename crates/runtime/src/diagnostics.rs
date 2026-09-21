//! Explicit support exports. Only typed health and fixed build metadata leave the host.
use cxweb_domain::health::{Component, Health};
use serde::Serialize;
use std::{io::Write, path::Path};

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    collector_version: &'static str,
    platform: &'static str,
    architecture: &'static str,
    health: Health,
}

fn timestamp(value: &str) -> bool {
    value.len() == 24
        && value.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            10 => byte == b'T',
            13 | 16 => byte == b':',
            19 => byte == b'.',
            23 => byte == b'Z',
            _ => byte.is_ascii_digit(),
        })
}

fn component(value: &mut Component) {
    value.observed_at = value.observed_at.take().filter(|value| timestamp(value));
    value.code = value
        .code
        .take()
        .map(|code| crate::health::export_code(&code).to_owned());
}

fn render(mut health: Health) -> Result<String, &'static str> {
    health.schema_version = "webbridge.health.v1".into();
    let parts = &mut health.components;
    for value in [
        &mut parts.runtime,
        &mut parts.browser,
        &mut parts.web_auth,
        &mut parts.web_models,
        &mut parts.native_upstream,
        &mut parts.codex_app,
        &mut parts.codex_cli,
        &mut parts.config,
    ] {
        component(value);
    }
    serde_json::to_string_pretty(&Report {
        schema: "cxweb.support.v1",
        collector_version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        health,
    })
    .map_err(|_| "E_DIAGNOSTICS_SERIALIZE")
}

/// Read-only private IPC; does not open browsers, generate, read logs or credentials.
pub async fn collect(installation: &str, instance: &str) -> Result<String, &'static str> {
    let snapshot = crate::installed_control::check(installation).await?;
    if snapshot.instance != instance {
        return Err("E_INSTALLED_CHANGED");
    }
    render(snapshot.health)
}

/// The desktop supplies a path returned by the native Save dialog, never a webview path.
/// Existing files (including links) are never overwritten by a support export.
pub fn write_new(path: &Path, report: &str) -> Result<(), &'static str> {
    if !path.is_absolute() {
        return Err("E_DIAGNOSTICS_DESTINATION");
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "E_DIAGNOSTICS_EXISTS"
            } else {
                "E_DIAGNOSTICS_WRITE"
            }
        })?;
    file.write_all(report.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "E_DIAGNOSTICS_WRITE")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cxweb_domain::health::{ComponentState, Evidence};

    #[test]
    fn exports_only_known_health_fields_and_codes_even_when_input_contains_secrets() {
        let mut health = Health {
            schema_version: "secret-schema".into(),
            ..Health::default()
        };
        health.components.web_auth = Component {
            state: ComponentState::AuthRequired,
            evidence: Evidence::LocalProbe,
            code: Some("E_BROWSER_VERIFICATION_REQUIRED".into()),
            observed_at: Some("2026-09-21T06:30:50.290Z".into()),
        };
        health.components.browser.code = Some("E_BROWSER_SECRET_SESSION_TOKEN".into());
        health.components.browser.observed_at =
            Some("person@example.test C:\\private\\project".into());
        let output = render(health).unwrap();
        for forbidden in [
            "secret-schema",
            "SECRET_SESSION_TOKEN",
            "person@",
            "private",
            "instance",
            "installation",
            "reasoning",
        ] {
            assert!(!output.contains(forbidden), "unexpected field: {forbidden}");
        }
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            value["health"]["components"]["browser"]["code"],
            "E_DIAGNOSTIC_REDACTED"
        );
        assert!(value["health"]["components"]["browser"]["observed_at"].is_null());
        assert_eq!(
            value["health"]["components"]["web_auth"]["code"],
            "E_BROWSER_VERIFICATION_REQUIRED"
        );
        assert_eq!(
            value["health"]["components"]["web_auth"]["observed_at"],
            "2026-09-21T06:30:50.290Z"
        );
    }

    #[test]
    fn explicit_export_preserves_existing_files_and_rejects_relative_destinations() {
        let path = std::env::temp_dir().join(format!(
            "cxweb-support-{:032x}.json",
            rand::random::<u128>()
        ));
        let report = render(Health::default()).unwrap();
        write_new(&path, &report).unwrap();
        assert_eq!(write_new(&path, "replacement"), Err("E_DIAGNOSTICS_EXISTS"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), report);
        assert_eq!(
            write_new(Path::new("relative.json"), &report),
            Err("E_DIAGNOSTICS_DESTINATION")
        );
        std::fs::remove_file(path).unwrap();
    }
}
