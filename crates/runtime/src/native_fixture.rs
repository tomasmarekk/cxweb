//! Exact operations for isolated native read/patch/test and denial diagnostics.
//! This policy never executes tools and is never installed on production routes.
use cxweb_codex_adapter::strict_json;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub(crate) const READ: &str = "Get-Content -LiteralPath './probe-input.txt'";
pub(crate) const DENIED: &str = "cxweb native read was denied";
pub(crate) const TEST_PASSED: &str = "cxweb fixture tests passed";
pub(crate) const TEST: &str = "if ((Get-Content -LiteralPath './probe-output.txt' -Raw -ErrorAction Stop) -ceq (Get-Content -LiteralPath './probe-input.txt' -Raw -ErrorAction Stop)) { Write-Output 'cxweb fixture tests passed'; exit 0 } else { Write-Output 'cxweb fixture tests failed'; exit 1 }";
const ERROR: &str = "E_NATIVE_PROBE_ACTION";

pub(crate) struct Fixture {
    pub denial: bool,
    pub test: bool,
    pub cwd: PathBuf,
    pub marker: String,
    shells: Vec<PathBuf>,
    deliveries: Mutex<BTreeMap<String, String>>,
}
impl Fixture {
    pub fn new(cwd: PathBuf, marker: String) -> Self {
        let mut shells = Vec::new();
        if let Some(paths) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&paths).filter(|p| p.is_absolute()) {
                for name in ["pwsh.exe", "powershell.exe"] {
                    let candidate = directory.join(name);
                    if candidate.is_file() {
                        shells.push(candidate);
                    }
                }
            }
        }
        for candidate in [
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
        ] {
            if Path::new(candidate).is_file() {
                shells.push(candidate.into());
            }
        }
        Self {
            denial: false,
            test: false,
            cwd,
            marker,
            shells,
            deliveries: Mutex::new(BTreeMap::new()),
        }
    }
    pub fn expected(&self) -> &str {
        if self.denial {
            DENIED
        } else if self.test {
            TEST_PASSED
        } else {
            &self.marker
        }
    }
    pub fn prompt(&self) -> String {
        if self.denial {
            return format!(
                "Use exec_command exactly once with cmd exactly {READ:?}, login=false and the current working directory. The user will deny this test command. Wait for the actual tool result. If the command is denied, return exactly: {DENIED}. Do not retry, run a different command, modify files, request elevated permissions or claim you read the file."
            );
        }
        if self.test {
            return format!(
                "Use exec_command with cmd exactly {READ:?}, login=false and the current working directory. Then use the apply_patch custom tool exactly once to add probe-output.txt containing the exact line read followed by a newline. After the patch succeeds, use exec_command with cmd exactly {TEST:?}, login=false and the current working directory. Wait for each actual tool result. Return exactly {TEST_PASSED:?} only if the test command exits with code 0 and prints that text. Do not claim the test passed before observing its result. Do not run other commands, change other files, request elevated permissions or access the network."
            );
        }
        format!(
            "Use exec_command exactly once with cmd exactly {READ:?}, login=false and the current working directory. Read probe-input.txt. Then use the apply_patch custom tool exactly once to add probe-output.txt with that exact line followed by a newline. Wait for the successful tool results. Do not use other tools, run other commands, change other files, request elevated permissions or access the network. Return exactly the line read from probe-input.txt as your final answer, without extra text."
        )
    }
    pub fn patch(&self) -> String {
        format!(
            "*** Begin Patch\n*** Add File: probe-output.txt\n+{}\n*** End Patch",
            self.marker
        )
    }
    /// Validate the complete buffered native response before any SSE/JSON bytes
    /// reach Codex. A rejected operation can therefore never be auto-executed.
    pub fn check_delivery(&self, json: &str) -> Result<(), &'static str> {
        let response = strict_json::parse(json.as_bytes(), 8 * 1024 * 1024).map_err(|_| ERROR)?;
        let id = response["id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .ok_or(ERROR)?;
        let hash = format!("{:x}", Sha256::digest(json.as_bytes()));
        let mut delivered = self.deliveries.lock().map_err(|_| ERROR)?;
        if let Some(previous) = delivered.get(id) {
            return if *previous == hash {
                Ok(())
            } else {
                Err(ERROR)
            };
        }
        let output = response["output"]
            .as_array()
            .filter(|v| v.len() == 1)
            .ok_or(ERROR)?;
        let item = &output[0];
        let valid = match delivered.len() {
            0 => {
                item["type"] == "function_call"
                    && item["name"] == "exec_command"
                    && namespace(item)
                    && item["arguments"]
                        .as_str()
                        .is_some_and(|a| self.command_arguments(a, READ))
            }
            1 if !self.denial => {
                item["type"] == "custom_tool_call"
                    && item["name"] == "apply_patch"
                    && namespace(item)
                    && item["input"]
                        .as_str()
                        .is_some_and(|s| s == self.patch() || s == self.patch() + "\n")
            }
            2 if self.test && !self.denial => {
                item["type"] == "function_call"
                    && item["name"] == "exec_command"
                    && namespace(item)
                    && item["arguments"]
                        .as_str()
                        .is_some_and(|a| self.command_arguments(a, TEST))
            }
            step if step
                == if self.denial {
                    1
                } else if self.test {
                    3
                } else {
                    2
                } =>
            {
                item["type"] == "message"
                    && item["role"] == "assistant"
                    && item["content"].as_array().is_some_and(|content| {
                        content.len() == 1
                            && content[0]["type"] == "output_text"
                            && content[0]["text"] == self.expected()
                    })
            }
            _ => false,
        };
        if response["status"] != "completed" || !valid {
            return Err(ERROR);
        }
        delivered.insert(id.into(), hash);
        Ok(())
    }
    fn command_arguments(&self, text: &str, command: &str) -> bool {
        let Ok(value) = strict_json::parse(text.as_bytes(), 16 * 1024) else {
            return false;
        };
        let Some(args) = value.as_object() else {
            return false;
        };
        args.keys().all(|k| {
            matches!(
                k.as_str(),
                "cmd" | "login" | "workdir" | "max_output_tokens" | "yield_time_ms"
            )
        }) && args.get("cmd").is_some_and(|v| v == command)
            && args.get("login").is_some_and(|v| v == false)
            && args
                .get("workdir")
                .is_none_or(|v| v.as_str().is_some_and(|p| same_path(p, &self.cwd)))
            && args
                .get("max_output_tokens")
                .is_none_or(|v| v.as_u64().is_some_and(|n| (1..=2048).contains(&n)))
            && args
                .get("yield_time_ms")
                .is_none_or(|v| v.as_u64().is_some_and(|n| (1000..=10000).contains(&n)))
    }
    pub fn approve_read(&self, params: &Value) -> bool {
        self.approve_command(params, READ)
    }
    pub fn approve_test(&self, params: &Value) -> bool {
        self.test && !self.denial && self.approve_command(params, TEST)
    }
    fn approve_command(&self, params: &Value, expected: &str) -> bool {
        if !params["cwd"]
            .as_str()
            .is_some_and(|p| same_path(p, &self.cwd))
            || params
                .get("kind")
                .is_some_and(|v| !v.is_null() && v != "command")
            || params
                .get("networkApprovalContext")
                .is_some_and(|v| !v.is_null())
        {
            return false;
        }
        let Some(command) = params["command"].as_str() else {
            return false;
        };
        if command == expected {
            return true;
        }
        let Some(argv) = words(command) else {
            return false;
        };
        if argv.len() < 3
            || argv.last().map(String::as_str) != Some(expected)
            || argv[argv.len() - 2] != "-Command"
            || !self.shells.iter().any(|p| same_path(&argv[0], p))
        {
            return false;
        }
        matches!(
            argv[1..argv.len() - 2].join(" ").as_str(),
            "" | "-NoProfile" | "-NoLogo -NoProfile" | "-NoProfile -NonInteractive"
        )
    }
    pub fn approve_patch(&self, params: &Value, started: &Value) -> bool {
        let item = &started["params"]["item"];
        params.get("grantRoot").is_none_or(Value::is_null)
            && params["itemId"].is_string()
            && params["itemId"] == item["id"]
            && params["threadId"] == started["params"]["threadId"]
            && params["turnId"] == started["params"]["turnId"]
            && item["type"] == "fileChange"
            && item["status"] == "inProgress"
            && self.patch_changes(item)
    }
    pub fn patch_changes(&self, item: &Value) -> bool {
        item["changes"].as_array().is_some_and(|changes| {
            changes.len() == 1
                && changes[0]["kind"]["type"] == "add"
                && changes[0]["path"].as_str().is_some_and(|p| {
                    p == "probe-output.txt" || same_path(p, &self.cwd.join("probe-output.txt"))
                })
                && changes[0]["diff"] == self.marker.clone() + "\n"
        })
    }
}
fn namespace(item: &Value) -> bool {
    item.get("namespace")
        .is_none_or(|v| v.is_null() || v == "functions")
}
fn same_path(value: &str, expected: &Path) -> bool {
    fn normalized(value: &str) -> String {
        let value = value.replace('\\', "/");
        value
            .strip_prefix("//?/")
            .unwrap_or(&value)
            .to_ascii_lowercase()
    }
    Path::new(value).is_absolute() && normalized(value) == normalized(&expected.to_string_lossy())
}
/// Bounded inverse of the reviewed backend's shlex rendering. No interpretation
/// of shell expansion, substitutions or operators takes place here.
fn words(value: &str) -> Option<Vec<String>> {
    if value.len() > 8192 || value.contains('\0') {
        return None;
    }
    let mut result = Vec::new();
    let mut quote = None;
    let mut word = String::new();
    let mut started = false;
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if quote == Some('\'') {
            if c == '\'' {
                quote = None;
            } else {
                word.push(c);
            }
        } else if c == '\\' {
            let next = chars.next()?;
            if quote == Some('"') && !matches!(next, '"' | '\\' | '$' | '`' | '\n') {
                word.push('\\');
            }
            if next != '\n' {
                word.push(next);
            }
            started = true;
        } else if quote == Some('"') {
            if c == '"' {
                quote = None;
            } else {
                word.push(c);
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
            started = true;
        } else if c.is_whitespace() {
            if started {
                result.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(c);
            started = true;
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        result.push(word);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> Fixture {
        Fixture::new(r"C:\fixture\workspace".into(), "private-marker".into())
    }
    fn response(id: &str, item: Value) -> String {
        json!({"id":id,"status":"completed","output":[item]}).to_string()
    }
    fn read(args: Value) -> Value {
        json!({"type":"function_call","name":"exec_command","arguments":args.to_string()})
    }
    #[test]
    fn delivery_policy_binds_exact_order_content_and_replay_identity() {
        let fixture = fixture();
        let first = response("read", read(json!({"cmd":READ,"login":false})));
        let patch = response(
            "patch",
            json!({"type":"custom_tool_call","name":"apply_patch","input":fixture.patch()}),
        );
        let final_text = response(
            "final",
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":fixture.marker}]}),
        );
        assert_eq!(fixture.check_delivery(&final_text), Err(ERROR));
        assert_eq!(fixture.check_delivery(&patch), Err(ERROR));
        for value in [&first, &patch, &final_text] {
            assert_eq!(fixture.check_delivery(value), Ok(()));
            assert_eq!(fixture.check_delivery(value), Ok(()));
        }
        assert_eq!(
            fixture.check_delivery(&first.replace("\"read\"", "\"other\"")),
            Err(ERROR)
        );
        assert_eq!(
            fixture.check_delivery(&final_text.replace("private-marker", "changed")),
            Err(ERROR)
        );
        assert_eq!(fixture.check_delivery(&first), Ok(()));
    }
    #[test]
    fn test_requires_read_then_patch_then_the_exact_command_before_a_final_claim() {
        let mut fixture = fixture();
        fixture.test = true;
        let first = response("read", read(json!({"cmd":READ,"login":false})));
        let patch = response(
            "patch",
            json!({"type":"custom_tool_call","name":"apply_patch","input":fixture.patch()}),
        );
        let test = response("test", read(json!({"cmd":TEST,"login":false})));
        let final_text = response(
            "final",
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":TEST_PASSED}]}),
        );
        assert!(fixture.check_delivery(&test).is_err());
        fixture.check_delivery(&first).unwrap();
        fixture.check_delivery(&patch).unwrap();
        assert!(fixture.check_delivery(&final_text).is_err());
        for cmd in [
            READ.to_owned(),
            format!("{TEST}; Get-ChildItem"),
            TEST.replace("exit 1", "exit 0"),
        ] {
            assert!(
                fixture
                    .check_delivery(&response("test", read(json!({"cmd":cmd,"login":false}))))
                    .is_err()
            );
        }
        fixture.check_delivery(&test).unwrap();
        fixture.check_delivery(&test).unwrap();
        fixture.check_delivery(&final_text).unwrap();
        assert!(fixture.approve_test(&json!({"cwd":fixture.cwd,"command":TEST})));
        assert!(!fixture.approve_test(&json!({"cwd":r"C:\other","command":TEST})));
        assert!(
            !fixture.approve_test(
                &json!({"cwd":fixture.cwd,"command":TEST,"networkApprovalContext":{}})
            )
        );
    }

    #[test]
    fn denial_allows_only_one_read_then_exact_acknowledgement() {
        let mut fixture = fixture();
        fixture.denial = true;
        assert!(!fixture.prompt().contains(&fixture.marker));
        let first = response("read", read(json!({"cmd":READ,"login":false})));
        let final_text = response(
            "final",
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":DENIED}]}),
        );
        assert_eq!(fixture.check_delivery(&final_text), Err(ERROR));
        assert_eq!(fixture.check_delivery(&first), Ok(()));
        assert_eq!(fixture.check_delivery(&first), Ok(()));
        assert_eq!(
            fixture.check_delivery(&response("retry", read(json!({"cmd":READ,"login":false})))),
            Err(ERROR)
        );
        assert_eq!(
            fixture.check_delivery(&response(
                "patch",
                json!({"type":"custom_tool_call","name":"apply_patch","input":fixture.patch()})
            )),
            Err(ERROR)
        );
        assert_eq!(
            fixture.check_delivery(&final_text.replace(DENIED, &fixture.marker)),
            Err(ERROR)
        );
        assert_eq!(fixture.check_delivery(&final_text), Ok(()));
        assert_eq!(fixture.check_delivery(&final_text), Ok(()));
        assert_eq!(
            fixture.check_delivery(&final_text.replace("final", "extra")),
            Err(ERROR)
        );
    }
    #[test]
    fn additional_commands_permissions_arguments_and_namespaces_never_reach_native_client() {
        for args in [
            json!({"cmd":format!("{READ}; Write-Output injected"),"login":false}),
            json!({"cmd":READ,"login":true}),
            json!({"cmd":READ}),
            json!({"cmd":READ,"login":false,"shell":"C:/untrusted/pwsh.exe"}),
            json!({"cmd":READ,"login":false,"sandbox_permissions":"require_escalated"}),
            json!({"cmd":READ,"login":false,"workdir":"C:/fixture/other"}),
            json!({"cmd":READ,"login":false,"workdir":"C:/fixture/workspace/../workspace"}),
            json!({"cmd":READ,"login":false,"yield_time_ms":0}),
            json!({"cmd":READ,"login":false,"max_output_tokens":999999}),
        ] {
            assert_eq!(
                fixture().check_delivery(&response("bad", read(args))),
                Err(ERROR)
            );
        }
        let mut value = read(json!({"cmd":READ,"login":false}));
        value["namespace"] = json!("unrelated");
        assert_eq!(
            fixture().check_delivery(&response("bad", value)),
            Err(ERROR)
        );
        let value = read(json!({"cmd":READ,"login":false}));
        let multiple = json!({"id":"bad","status":"completed","output":[value,value]});
        assert_eq!(fixture().check_delivery(&multiple.to_string()), Err(ERROR));
        let duplicate = response(
            "bad",
            json!({"type":"function_call","name":"exec_command","arguments":format!("{{\"cmd\":{0},\"cmd\":{0},\"login\":false}}", json!(READ))}),
        );
        assert_eq!(fixture().check_delivery(&duplicate), Err(ERROR));
    }
    #[test]
    fn changed_patch_paths_or_content_are_refused_before_delivery() {
        for patch in [
            "*** Begin Patch\n*** Delete File: probe-input.txt\n*** End Patch".to_owned(),
            fixture()
                .patch()
                .replace("probe-output.txt", "../probe-output.txt"),
            fixture().patch().replace("private-marker", "changed"),
            fixture().patch().replace(
                "*** End Patch",
                "*** Add File: another.txt\n+injected\n*** End Patch",
            ),
        ] {
            let fixture = fixture();
            fixture
                .check_delivery(&response("read", read(json!({"cmd":READ,"login":false}))))
                .unwrap();
            assert_eq!(
                fixture.check_delivery(&response(
                    "patch",
                    json!({"type":"custom_tool_call","name":"apply_patch","input":patch})
                )),
                Err(ERROR)
            );
            assert_eq!(fixture.deliveries.lock().unwrap().len(), 1);
        }
    }
    #[test]
    fn approvals_bind_exact_shell_directory_item_and_patch() {
        let mut fixture = fixture();
        fixture.shells = vec![r"C:\trusted\pwsh.exe".into()];
        for command in [
            READ.to_owned(),
            format!("'C:\\trusted\\pwsh.exe' -NoProfile -Command \"{READ}\""),
        ] {
            assert!(fixture.approve_read(&json!({"cwd":fixture.cwd,"command":command})));
        }
        for command in [
            format!("{READ}; Write-Output injected"),
            format!("'C:\\other\\pwsh.exe' -Command \"{READ}\""),
            format!("'C:\\trusted\\pwsh.exe' -Command \"{READ}\" -File other.ps1"),
        ] {
            assert!(!fixture.approve_read(&json!({"cwd":fixture.cwd,"command":command})));
        }
        assert!(
            !fixture.approve_read(
                &json!({"cwd":fixture.cwd,"command":READ,"networkApprovalContext":{}})
            )
        );
        let event = json!({"params":{"threadId":"thread","turnId":"turn","item":{"id":"item","type":"fileChange","status":"inProgress","changes":[{"kind":{"type":"add"},"path":"probe-output.txt","diff":"private-marker\n"}]}}});
        let params = json!({"threadId":"thread","turnId":"turn","itemId":"item","grantRoot":null});
        assert!(fixture.approve_patch(&params, &event));
        for key in ["threadId", "turnId", "itemId", "grantRoot"] {
            let mut altered = params.clone();
            altered[key] = json!("other");
            assert!(!fixture.approve_patch(&altered, &event));
        }
    }
}
