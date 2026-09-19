//! Browser operations for the durable coordinator, serialized on the browser owner.
//! The account/workspace verifier reads the browser UI at turn boundaries.
use crate::browser_scope::BrowserScope;
use crate::turn::{BrowserDriver, BrowserFuture, Prepared};
use cxweb_browser_adapter::{
    ManagedBrowser, ManagedPage,
    turn::{Baseline, Observation},
};
use cxweb_domain::SessionKey;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

pub struct Route {
    pub id: String,
    pub identity: String,
    pub label: String,
    pub effort: Option<String>,
}

pub struct Binding {
    pub installation: String,
    pub account: String,
    pub workspace: String,
    pub epoch: u64,
    pub routes: Vec<Route>,
}

impl Binding {
    fn route(&self, session: &SessionKey) -> Result<&Route, &'static str> {
        if session.installation != self.installation
            || session.account_scope != self.account
            || session.workspace_scope != self.workspace
            || session.epoch != self.epoch
            || session.native_session.is_empty()
            || session.native_session.len() > 512
            || session.native_session.chars().any(char::is_control)
        {
            return Err("E_SESSION_SCOPE");
        }
        self.routes
            .iter()
            .find(|route| route.id == session.route)
            .ok_or("E_MODEL_UNAVAILABLE")
    }
    fn validate(&self) -> Result<(), &'static str> {
        let valid = |value: &str| {
            !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
        };
        if ![&self.installation, &self.account, &self.workspace]
            .iter()
            .all(|value| valid(value))
            || self.routes.is_empty()
            || self.routes.len() > 256
        {
            return Err("E_SESSION_SCOPE");
        }
        let mut ids = std::collections::HashSet::new();
        for route in &self.routes {
            if !valid(&route.id)
                || !route.id.starts_with("webbridge/")
                || route.id == "webbridge/"
                || !valid(&route.identity)
                || !valid(&route.label)
                || !ids.insert(&route.id)
            {
                return Err("E_MODEL_UNAVAILABLE");
            }
        }
        Ok(())
    }
}

type ScopeVerifier = Box<
    dyn FnMut(&mut ManagedBrowser, &ManagedPage) -> Result<(String, String), &'static str> + Send,
>;
type Reply<T> = oneshot::Sender<Result<T, &'static str>>;
enum Command {
    Prepare(SessionKey, Reply<Prepared>),
    Submit(String, String, String, Reply<()>),
    Observe(String, Reply<Observation>),
    VerifyCompletion(String, Reply<()>),
    Stop(String, Reply<bool>),
    Release(String, Reply<()>),
    Shutdown(Reply<()>),
    Diagnostic(Reply<serde_json::Value>),
}
struct Lease {
    session: SessionKey,
    page: ManagedPage,
    baseline: Baseline,
    prompt: Option<String>,
    attempted: bool,
}

#[derive(Clone)]
pub struct ManagedDriver {
    commands: mpsc::Sender<Command>,
}

impl ManagedDriver {
    /// Takes ownership of the already authenticated browser. Its private pipe
    /// must have exactly one owner; login control cannot keep another handle.
    pub fn start(
        mut browser: ManagedBrowser,
        binding: Binding,
        ownership: std::fs::File,
    ) -> Result<Self, &'static str> {
        binding.validate()?;
        let installation = binding.installation.clone();
        let mut verify: ScopeVerifier = Box::new(move |browser, page| {
            let surface = browser.account_scope(page).map_err(|_| "E_SESSION_SCOPE")?;
            let scope = BrowserScope::from_surface(&installation, &surface)?;
            Ok((scope.account, scope.workspace))
        });
        let (commands, mut incoming) = mpsc::channel(32);
        std::thread::Builder::new()
            .name("cxweb-generation-browser".into())
            .spawn(move || {
                // The instance/profile lock outlives desktop windows and is
                // released only after the browser owner has stopped.
                let _ownership = ownership;
                let mut leases = HashMap::<String, Lease>::new();
                let mut orphaned = Vec::<ManagedPage>::new();
                let mut shutdown_reply = None;
                let mut output_shape = serde_json::Value::Null;
                let check_scope = |browser: &mut ManagedBrowser,
                                   page: &ManagedPage,
                                   verify: &mut ScopeVerifier| {
                    let (account, workspace) = verify(browser, page)?;
                    if account != binding.account || workspace != binding.workspace {
                        return Err("E_SESSION_SCOPE");
                    }
                    Ok(())
                };
                while let Some(command) = incoming.blocking_recv() {
                    match command {
                        Command::Diagnostic(reply) => {
                            let _ = reply.send(Ok(serde_json::json!({"attribution":browser.attribution_diagnostic(), "scope":browser.scope_diagnostic(), "model":browser.model_diagnostic(), "output_shape":output_shape})));
                        }
                        Command::Shutdown(reply) => {
                            shutdown_reply = Some(reply);
                            break;
                        }
                        Command::Prepare(session, reply) => {
                            if reply.is_closed() {
                                continue;
                            }
                            orphaned.retain(|page| browser.close_page_checked(page).is_err());
                            let result = (|| {
                                let route = binding.route(&session)?;
                                if leases.len() + orphaned.len() >= 8
                                    || leases.values().any(|lease| lease.session == session)
                                {
                                    return Err("E_BROWSER_BUSY");
                                }
                                let page = browser
                                    .open_temporary_chat()
                                    .map_err(|_| "E_TEMPORARY_CHAT")?;
                                let prepared = (|| {
                                    check_scope(&mut browser, &page, &mut verify)?;
                                    let label = browser
                                        .select_candidate(&page, &route.identity)
                                        .map_err(|_| "E_MODEL_SELECTION")?;
                                    if label != route.label {
                                        return Err("E_MODEL_SELECTION");
                                    }
                                    let baseline = browser
                                        .baseline(&page)
                                        .map_err(|_| "E_BROWSER_BASELINE")?;
                                    if !baseline.composer_empty || baseline.generating {
                                        return Err("E_BROWSER_BUSY");
                                    }
                                    Ok(baseline)
                                })();
                                let baseline = match prepared {
                                    Ok(baseline) => baseline,
                                    Err(code) => {
                                        if browser.close_page_checked(&page).is_err() {
                                            orphaned.push(page);
                                        }
                                        return Err(code);
                                    }
                                };
                                let handle = format!("{:032x}", rand::random::<u128>());
                                let prepared = Prepared {
                                    handle: handle.clone(),
                                    verified_session: session.clone(),
                                    baseline: baseline.clone(),
                                    verified_route: route.id.clone(),
                                    verified_effort: route.effort.clone(),
                                };
                                leases.insert(
                                    handle,
                                    Lease {
                                        session,
                                        page,
                                        baseline,
                                        prompt: None,
                                        attempted: false,
                                    },
                                );
                                Ok(prepared)
                            })();
                            if let Err(Ok(prepared)) = reply.send(result)
                                && let Some(lease) = leases.remove(&prepared.handle)
                                && browser.close_page_checked(&lease.page).is_err()
                            {
                                orphaned.push(lease.page);
                            }
                        }
                        Command::Submit(handle, prompt, selected_model, reply) => {
                            if reply.is_closed() {
                                continue;
                            }
                            let result = (|| {
                                let lease = leases.get_mut(&handle).ok_or("E_BROWSER_LEASE")?;
                                if lease.attempted {
                                    return Err("E_SUBMISSION_ALREADY_ATTEMPTED");
                                }
                                // Latch before any preparation: a partial failure is never resent.
                                lease.attempted = true;
                                check_scope(&mut browser, &lease.page, &mut verify)?;
                                if selected_model != lease.baseline.selected_model {
                                    return Err("E_MODEL_SELECTION");
                                }
                                browser
                                    .insert_prompt(&lease.page, &prompt)
                                    .map_err(|_| "E_COMPOSER_INSERT")?;
                                let route = binding.route(&lease.session)?;
                                browser
                                    .verify_candidate(&lease.page, &route.identity, &route.label)
                                    .map_err(|_| "E_MODEL_SELECTION")?;
                                lease.prompt = Some(prompt);
                                browser
                                    .press_send(
                                        &lease.page,
                                        lease.prompt.as_ref().unwrap(),
                                        &selected_model,
                                    )
                                    .map_err(|_| "E_SUBMISSION_UNCERTAIN")
                            })();
                            let _ = reply.send(result);
                        }
                        Command::Observe(handle, reply) => {
                            if reply.is_closed() {
                                continue;
                            }
                            let result = (|| {
                                let lease = leases.get(&handle).ok_or("E_BROWSER_LEASE")?;
                                browser
                                    .observe(
                                        &lease.page,
                                        &lease.baseline,
                                        lease.prompt.as_ref().ok_or("E_TURN_STATE")?,
                                    )
                                    .map_err(|_| "E_BROWSER_OBSERVATION")
                            })();
                            if let Ok(observation) = &result {
                                output_shape = summarize_output_shape(&observation.text);
                            }
                            let _ = reply.send(result);
                        }
                        Command::VerifyCompletion(handle, reply) => {
                            if reply.is_closed() {
                                continue;
                            }
                            let result = (|| {
                                let lease = leases.get(&handle).ok_or("E_BROWSER_LEASE")?;
                                if !lease.attempted || lease.prompt.is_none() {
                                    return Err("E_TURN_STATE");
                                }
                                check_scope(&mut browser, &lease.page, &mut verify)
                            })();
                            let _ = reply.send(result);
                        }
                        Command::Stop(handle, reply) => {
                            // Cleanup still runs if the waiting HTTP future was dropped.
                            let result = match leases.get(&handle) {
                                Some(lease) => browser
                                    .stop(&lease.page)
                                    .and_then(|_| browser.baseline(&lease.page))
                                    .map(|baseline| !baseline.generating)
                                    .map_err(|_| "E_BROWSER_STOP"),
                                None => Err("E_BROWSER_LEASE"),
                            };
                            let _ = reply.send(result);
                        }
                        Command::Release(handle, reply) => {
                            let result = match leases.get(&handle) {
                                Some(lease) => {
                                    let _ = browser.stop(&lease.page);
                                    browser
                                        .close_page_checked(&lease.page)
                                        .map_err(|_| "E_BROWSER_RELEASE")
                                }
                                None => Ok(()),
                            };
                            if result.is_ok() {
                                leases.remove(&handle);
                            }
                            let _ = reply.send(result);
                        }
                    }
                }
                for (_, lease) in leases {
                    let _ = browser.stop(&lease.page);
                    let _ = browser.close_page(lease.page);
                }
                for page in orphaned {
                    let _ = browser.close_page(page);
                }
                let closed = browser.close().map_err(|_| "E_BROWSER_RELEASE");
                drop(browser);
                drop(_ownership);
                if let Some(reply) = shutdown_reply {
                    let _ = reply.send(closed);
                }
            })
            .map_err(|_| "E_BROWSER_WORKER")?;
        Ok(Self { commands })
    }

    /// Call after draining admitted requests. Completion proves browser exit and
    /// release of the dedicated profile lock, not merely a dropped UI handle.
    pub fn shutdown(&self) -> BrowserFuture<()> {
        self.request(Command::Shutdown)
    }

    pub fn diagnostic(&self) -> BrowserFuture<serde_json::Value> {
        self.request(Command::Diagnostic)
    }

    fn request<T: Send + 'static>(
        &self,
        command: impl FnOnce(Reply<T>) -> Command + Send + 'static,
    ) -> BrowserFuture<T> {
        let commands = self.commands.clone();
        Box::pin(async move {
            let (reply, receive) = oneshot::channel();
            commands
                .try_send(command(reply))
                .map_err(|_| "E_BROWSER_BUSY")?;
            receive.await.map_err(|_| "E_BROWSER_CLOSED")?
        })
    }
}

fn summarize_output_shape(text: &str) -> serde_json::Value {
    use serde_json::json;
    let parsed = cxweb_codex_adapter::strict_json::parse(text.as_bytes(), 8 * 1024 * 1024).ok();
    let object = parsed.as_ref().and_then(serde_json::Value::as_object);
    let summary_text = parsed.as_ref().and_then(|value| value["summary"].as_str());
    let summary = summary_text
        .and_then(|text| cxweb_codex_adapter::strict_json::parse(text.as_bytes(), 256 * 1024).ok());
    let summary_fields = [
        "constraints",
        "changed_files",
        "decisions",
        "outstanding_work",
        "test_results",
        "unresolved_tool_ids",
    ]
    .into_iter()
    .map(|field| {
        (
            field.to_string(),
            json!(
                summary
                    .as_ref()
                    .and_then(|value| value[field].as_array())
                    .is_some_and(|items| items.iter().all(serde_json::Value::is_string))
            ),
        )
    })
    .collect::<serde_json::Map<_, _>>();
    json!({
        "json_valid":parsed.is_some(),
        "starts_object":text.trim_start().starts_with('{'),
        "unicode_quote_escape":text.contains("\\u0022"),
        "unescaped_nested_text":text.contains("\"text\":\"{\"") || text.contains("\"text\": \"{\""),
        "object":object.is_some(),
        "field_count":object.map(|value| value.len()),
        "protocol_valid":parsed.as_ref().is_some_and(|value| value["protocol"] == "webbridge.tool.v1"),
        "kind_final":parsed.as_ref().is_some_and(|value| value["kind"] == "final"),
        "text_string":parsed.as_ref().is_some_and(|value| value["text"].is_string()),
        "text_object":parsed.as_ref().is_some_and(|value| value["text"].is_object()),
        "title_string":parsed.as_ref().is_some_and(|value| value["title"].is_string()),
        "summary_string":summary_text.is_some(),
        "summary_json_valid":summary.is_some(),
        "summary_starts_object":summary_text.is_some_and(|text| text.trim_start().starts_with('{')),
        "summary_field_count":summary.as_ref().and_then(serde_json::Value::as_object).map(|object| object.len()),
        "summary_goal_string":summary.as_ref().is_some_and(|value| value["goal"].is_string()),
        "summary_string_arrays":summary_fields,
        "nonce_hex":parsed.as_ref().and_then(|value| value["turn_nonce"].as_str()).is_some_and(|value| value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())),
    })
}

impl BrowserDriver for ManagedDriver {
    fn verify_completion(&self, handle: String) -> BrowserFuture<()> {
        self.request(move |reply| Command::VerifyCompletion(handle, reply))
    }
    fn prepare(&self, session: SessionKey) -> BrowserFuture<Prepared> {
        self.request(move |reply| Command::Prepare(session, reply))
    }
    fn submit(&self, handle: String, prompt: String, selected_model: String) -> BrowserFuture<()> {
        self.request(move |reply| Command::Submit(handle, prompt, selected_model, reply))
    }
    fn observe(&self, handle: String) -> BrowserFuture<Observation> {
        self.request(move |reply| Command::Observe(handle, reply))
    }
    fn stop(&self, handle: String) -> BrowserFuture<bool> {
        self.request(move |reply| Command::Stop(handle, reply))
    }
    fn release(&self, handle: String) -> BrowserFuture<()> {
        self.request(move |reply| Command::Release(handle, reply))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_shape_exports_only_fixed_structural_metadata() {
        let observed = summarize_output_shape(
            r#"{"title":"PRIVATE_TITLE","PRIVATE_FIELD":"PRIVATE_CONTENT"}"#,
        );
        assert_eq!(observed["title_string"], true);
        assert_eq!(observed["field_count"], 2);
        assert_eq!(observed["protocol_valid"], false);
        assert!(!observed.to_string().contains("PRIVATE"));
        let observed = summarize_output_shape(&serde_json::json!({"kind":"checkpoint","summary":serde_json::json!({"goal":"PRIVATE_GOAL","PRIVATE_FIELD":"PRIVATE_DATA","decisions":["PRIVATE_DECISION"]}).to_string()}).to_string());
        assert_eq!(observed["summary_json_valid"], true);
        assert_eq!(observed["summary_field_count"], 3);
        assert_eq!(observed["summary_string_arrays"]["decisions"], true);
        assert_eq!(observed["summary_string_arrays"]["constraints"], false);
        assert!(!observed.to_string().contains("PRIVATE"));
        assert_eq!(
            summarize_output_shape("PRIVATE_NON_JSON")["json_valid"],
            false
        );
    }

    fn binding() -> Binding {
        Binding {
            installation: "installation".into(),
            account: "account".into(),
            workspace: "workspace".into(),
            epoch: 3,
            routes: vec![Route {
                id: "webbridge/fixture".into(),
                identity: "reasoning-slider:0:4:3".into(),
                label: "Observed route".into(),
                effort: None,
            }],
        }
    }
    fn session() -> SessionKey {
        SessionKey {
            installation: "installation".into(),
            native_session: "thread".into(),
            account_scope: "account".into(),
            workspace_scope: "workspace".into(),
            route: "webbridge/fixture".into(),
            epoch: 3,
        }
    }

    #[test]
    fn every_external_scope_dimension_and_route_is_checked_before_preparation() {
        let binding = binding();
        binding.validate().unwrap();
        assert!(binding.route(&session()).is_ok());
        for dimension in 0..7 {
            let mut session = session();
            match dimension {
                0 => session.installation = "other".into(),
                1 => session.account_scope = "other".into(),
                2 => session.workspace_scope = "other".into(),
                3 => session.epoch += 1,
                4 => session.route = "webbridge/unpublished".into(),
                5 => session.native_session.clear(),
                _ => session.native_session = "x".repeat(513),
            }
            assert!(binding.route(&session).is_err());
        }
    }

    #[test]
    fn activation_rejects_duplicate_or_foreign_routes() {
        let mut duplicate = binding();
        duplicate.routes.push(Route {
            id: "webbridge/fixture".into(),
            identity: "different".into(),
            label: "Different".into(),
            effort: None,
        });
        assert!(duplicate.validate().is_err());
        let mut foreign = binding();
        foreign.routes[0].id = "native".into();
        assert!(foreign.validate().is_err());
    }

    #[tokio::test]
    async fn dropped_cleanup_waiter_does_not_remove_queued_cleanup() {
        let (commands, mut incoming) = mpsc::channel(1);
        let driver = ManagedDriver { commands };
        let wait = tokio::spawn(driver.release("owned-handle".into()));
        let command = incoming.recv().await.unwrap();
        wait.abort();
        let _ = wait.await;
        match command {
            Command::Release(handle, reply) => {
                assert_eq!(handle, "owned-handle");
                assert!(reply.is_closed());
                // The command, including its owned handle, still belongs to
                // the worker even though it can no longer deliver a reply.
            }
            _ => panic!("wrong operation"),
        }
    }

    #[tokio::test]
    async fn overload_rejects_without_queueing_a_second_operation() {
        let (commands, mut incoming) = mpsc::channel(1);
        let driver = ManagedDriver { commands };
        let (reply, _) = oneshot::channel();
        driver
            .commands
            .try_send(Command::Release("first".into(), reply))
            .unwrap_or_else(|_| panic!("queue unexpectedly full"));
        assert_eq!(
            driver
                .submit("other".into(), "prompt".into(), "route".into())
                .await
                .err(),
            Some("E_BROWSER_BUSY")
        );
        assert!(matches!(
            incoming.recv().await,
            Some(Command::Release(_, _))
        ));
        assert!(incoming.try_recv().is_err());
    }
}
