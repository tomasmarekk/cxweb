//! Browser operations for the durable coordinator, serialized on the browser owner.
//! Activation must supply an independently qualified account/workspace verifier.
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

/// Reads the current browser account and workspace scope. It must not simply
/// echo Binding: activation supplies the DOM-backed verifier after qualification.
pub type ScopeVerifier = Box<
    dyn FnMut(&mut ManagedBrowser, &ManagedPage) -> Result<(String, String), &'static str> + Send,
>;
type Reply<T> = oneshot::Sender<Result<T, &'static str>>;
enum Command {
    Prepare(SessionKey, Reply<Prepared>),
    Submit(String, String, String, Reply<()>),
    Observe(String, Reply<Observation>),
    Stop(String, Reply<bool>),
    Release(String, Reply<()>),
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
        mut verify: ScopeVerifier,
        ownership: std::fs::File,
    ) -> Result<Self, &'static str> {
        binding.validate()?;
        let (commands, mut incoming) = mpsc::channel(32);
        std::thread::Builder::new()
            .name("cxweb-generation-browser".into())
            .spawn(move || {
                // The instance/profile lock outlives desktop windows and is
                // released only after the browser owner has stopped.
                let _ownership = ownership;
                let mut leases = HashMap::<String, Lease>::new();
                let mut orphaned = Vec::<ManagedPage>::new();
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
                                check_scope(&mut browser, &lease.page, &mut verify)?;
                                browser
                                    .observe(
                                        &lease.page,
                                        &lease.baseline,
                                        lease.prompt.as_ref().ok_or("E_TURN_STATE")?,
                                    )
                                    .map_err(|_| "E_BROWSER_OBSERVATION")
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
                let _ = browser.close();
            })
            .map_err(|_| "E_BROWSER_WORKER")?;
        Ok(Self { commands })
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

impl BrowserDriver for ManagedDriver {
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
