use crate::turn::{Baseline, Observation, Progress, TurnTracker};
use cxweb_platform::browser_process::{self, BrowserProcess};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, BufReader, Write},
    path::Path,
    sync::mpsc::{Receiver, sync_channel},
    time::{Duration, Instant},
};

const MAX_FRAME: usize = 8 * 1024 * 1024;

pub struct ManagedPage {
    target: String,
    session: String,
    fixture: bool,
    hidden: bool,
}

impl ManagedPage {
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginObservation {
    #[serde(default)]
    pub verification_required: bool,
    #[serde(default)]
    pub document_ready: bool,
    #[serde(default)]
    pub browser_language: Option<String>,
    #[serde(default)]
    pub page_language: Option<String>,
    pub official_page: bool,
    pub composer: bool,
    pub account_surface: bool,
    pub login_action: bool,
    pub selected_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCandidate {
    pub label: String,
    pub identity: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSurface {
    pub candidates: Vec<ModelCandidate>,
    pub temporary_chat: bool,
    pub diagnostic: ModelSurfaceDiagnostic,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSurfaceDiagnostic {
    #[serde(default)]
    pub read_failure: Option<String>,
    #[serde(default)]
    pub families: Vec<ModelFamily>,
    #[serde(default)]
    pub menu_options: Vec<ModelCandidate>,
    #[serde(default)]
    pub effort_range: Option<EffortRange>,
    #[serde(default)]
    pub menu_states: Vec<String>,
    #[serde(default)]
    pub interaction_state: std::collections::BTreeMap<String, bool>,
    #[serde(default)]
    pub composer_controls: Vec<String>,
    pub switcher_expanded: Option<bool>,
    pub visible_roots: usize,
    pub candidate_nodes: usize,
    pub model_testids: Vec<String>,
    pub visible_roles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFamily {
    pub label: String,
    pub identity: String,
    pub selected: bool,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffortRange {
    pub min: i64,
    pub max: i64,
    pub current: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualificationOutcome {
    pub candidate_label: String,
    pub response: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeDiagnostic {
    #[serde(default)]
    pub default_context_verified: bool,
    #[serde(default)]
    pub account_switcher_matches_settings: bool,
    #[serde(default)]
    pub switcher: Option<AccountSwitcherDiagnostic>,
    #[serde(default)]
    pub menu_controls: Vec<String>,
    #[serde(default)]
    pub workspace_markers: Vec<String>,
    #[serde(default)]
    pub settings_available: bool,
    #[serde(default)]
    pub settings_opened: bool,
    #[serde(default)]
    pub settings_controls: Vec<String>,
    #[serde(default)]
    pub settings_account_candidates: usize,
    #[serde(default)]
    pub settings_account_selected: bool,
    #[serde(default)]
    pub settings_panel_present: bool,
    #[serde(default)]
    pub settings_fields: Vec<String>,
    #[serde(default)]
    pub settings_loading: bool,
    #[serde(default)]
    pub failure: Option<String>,
    #[serde(default)]
    pub profile_control_tags: Vec<String>,
    pub menu_present: bool,
    pub account_candidates: usize,
    pub workspace_candidates: usize,
    pub selected_workspace_candidates: usize,
    pub menu_items: usize,
    pub selected_items: usize,
    pub has_account_id: bool,
    pub has_workspace_id: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSwitcherDiagnostic {
    #[serde(default)]
    pub account_source: Option<String>,
    #[serde(default)]
    pub selected_email_sources: Vec<usize>,
    #[serde(default)]
    pub email_controls: Vec<String>,
    pub available: bool,
    pub expanded: bool,
    pub submenu_present: bool,
    pub account_candidates: usize,
    pub workspace_candidates: usize,
    pub selected_items: usize,
    pub selected_account_candidates: usize,
    pub public_labels: Vec<String>,
    pub controls: Vec<String>,
}

// Identifiers stay in memory for comparison/hashing, never diagnostic output.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeSurface {
    pub account: Option<String>,
    pub workspace: Option<String>,
    /// An explicitly qualified UI-default context, not a provider workspace ID.
    /// Only the Rust evidence check may set this; DOM payloads cannot supply it.
    #[serde(skip)]
    pub default_workspace: bool,
    pub diagnostic: ScopeDiagnostic,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationDiagnostic {
    pub expected_model_label: String,
    pub observed_model_label: String,
    pub user_present: bool,
    pub user_matches: bool,
    pub assistant_present: bool,
    pub generating: bool,
}

pub struct ManagedBrowser {
    process: BrowserProcess,
    replies: Receiver<io::Result<Value>>,
    next_id: u64,
    failed_qualification: Option<ManagedPage>,
    qualification_diagnostic: Option<QualificationDiagnostic>,
    model_diagnostic: Option<ModelSurfaceDiagnostic>,
    login_keeper: Option<ManagedPage>,
    headless: bool,
    offscreen: bool,
    offscreen_started: bool,
}

impl ManagedBrowser {
    pub fn launch(executable: &Path, profile: &Path, visible: bool) -> io::Result<Self> {
        let process = browser_process::launch(executable, profile, visible)?;
        Self::from_process(process, !visible, false)
    }

    pub fn launch_offscreen(executable: &Path, profile: &Path) -> io::Result<Self> {
        Self::from_process(
            browser_process::launch_offscreen(executable, profile)?,
            false,
            true,
        )
    }

    fn from_process(process: BrowserProcess, headless: bool, offscreen: bool) -> io::Result<Self> {
        let output = process.output.try_clone()?;
        let (sender, replies) = sync_channel(32);
        std::thread::spawn(move || {
            let mut input = BufReader::new(output);
            loop {
                let result = read_frame(&mut input).and_then(|frame| {
                    serde_json::from_slice(&frame)
                        .map_err(|_| io::Error::other("invalid CDP frame"))
                });
                let failed = result.is_err();
                if sender.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            process,
            replies,
            next_id: 0,
            failed_qualification: None,
            qualification_diagnostic: None,
            model_diagnostic: None,
            login_keeper: None,
            headless,
            offscreen,
            offscreen_started: false,
        })
    }

    pub fn pid(&self) -> u32 {
        self.process.pid()
    }

    pub fn version(&mut self) -> io::Result<Value> {
        self.call("Browser.getVersion", json!({}), None)
    }

    pub fn open_login(&mut self) -> io::Result<ManagedPage> {
        if self.headless || self.offscreen {
            return Err(io::Error::other("E_VISIBLE_LOGIN_REQUIRED"));
        }
        // Keep the managed browser alive when the user closes its last visible
        // login window. The empty target neither reads nor automates sign-in.
        let value = self.call(
            "Target.createTarget",
            json!({"url":"https://chatgpt.com/", "newWindow":true}),
            None,
        )?;
        let target = value["targetId"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing target identity"))?;
        let page = self.attach(target.to_owned(), false, false)?;
        if self.login_keeper.is_none() {
            match self.open_hidden_page("about:blank", false) {
                Ok(keeper) => self.login_keeper = Some(keeper),
                Err(error) => {
                    let _ = self.close_page(page);
                    return Err(error);
                }
            }
        }
        Ok(page)
    }

    fn open_hidden_page(&mut self, url: &'static str, fixture: bool) -> io::Result<ManagedPage> {
        let params = if self.offscreen {
            if self.offscreen_started {
                json!({"url":"about:blank","background":true})
            } else {
                let (x, y, width, height) = BrowserProcess::background_bounds();
                json!({"url":"about:blank","background":true,"newWindow":true,"left":x,"top":y,"width":width,"height":height})
            }
        } else {
            json!({"url":"about:blank","hidden":!self.headless,"background":true})
        };
        let result = self
            .call("Target.createTarget", params, None)
            .map_err(|_| io::Error::other("E_HIDDEN_TARGET"))?;
        let target = result["targetId"]
            .as_str()
            .ok_or_else(|| io::Error::other("E_BROWSER_TARGET"))?
            .to_owned();
        if self.offscreen {
            if self.process.park_windows().is_err() {
                let _ = self.call("Target.closeTarget", json!({"targetId":target}), None);
                return Err(io::Error::other("E_BACKGROUND_WINDOW"));
            }
            self.offscreen_started = true;
        }
        match self.attach(target.clone(), fixture, true) {
            Ok(page) => {
                if url != "about:blank" {
                    let navigated =
                        self.call("Page.navigate", json!({"url":url}), Some(&page.session));
                    if !navigated.is_ok_and(|result| result.get("errorText").is_none()) {
                        let _ = self.close_page(page);
                        return Err(io::Error::other("E_BACKGROUND_NAVIGATION"));
                    }
                }
                Ok(page)
            }
            Err(error) => {
                let _ = self.call("Target.closeTarget", json!({"targetId":target}), None);
                Err(io::Error::other(
                    if error.to_string() == "E_HIDDEN_VIEWPORT" {
                        "E_HIDDEN_VIEWPORT"
                    } else {
                        "E_HIDDEN_ATTACH"
                    },
                ))
            }
        }
    }

    pub fn page_exists(&mut self, page: &ManagedPage) -> io::Result<bool> {
        let result = self.call("Target.getTargets", json!({}), None)?;
        let targets = result["targetInfos"]
            .as_array()
            .ok_or_else(|| io::Error::other("E_BROWSER_TARGETS"))?;
        Ok(targets
            .iter()
            .any(|target| target["targetId"] == page.target))
    }

    pub fn close_login_window(&mut self, page: &ManagedPage) -> io::Result<()> {
        if page.hidden {
            return Ok(());
        }
        let observation = self.login_observation(page)?;
        if !observation.official_page
            || !observation.composer
            || !observation.account_surface
            || observation.login_action
            || observation.verification_required
        {
            return Err(io::Error::other("E_LOGIN_REQUIRED"));
        }
        let baseline = self.baseline(page)?;
        if !baseline.composer_empty || baseline.generating {
            return Err(io::Error::other("E_BROWSER_BUSY"));
        }
        self.close_page_checked(page)
    }

    /// Resume the saved session after the user closes the sign-in window.
    /// Authentication is observed only on the official ChatGPT page; a login
    /// challenge is never handled automatically in the background.
    pub fn open_background_session(&mut self) -> io::Result<ManagedPage> {
        // Loading is observed by later status operations. A slow navigation
        // must not destroy a live target or repeatedly create replacement tabs.
        self.open_hidden_page("https://chatgpt.com/", false)
    }

    pub fn probe_startup_page(&mut self) -> io::Result<Value> {
        let targets = self.call("Target.getTargets", json!({}), None)?;
        let pages = targets["targetInfos"]
            .as_array()
            .ok_or_else(|| io::Error::other("missing target list"))?;
        let page_count = pages
            .iter()
            .filter(|target| target["type"] == "page")
            .count();
        if page_count != 0 {
            return Err(io::Error::other("E_BROWSER_EXTRA_STARTUP_PAGE"));
        }
        Ok(json!({"page_count":page_count,"startup_window_suppressed":true}))
    }
    fn attach(&mut self, target: String, fixture: bool, hidden: bool) -> io::Result<ManagedPage> {
        let result = self.call(
            "Target.attachToTarget",
            json!({"targetId":target,"flatten":true}),
            None,
        )?;
        let session = result["sessionId"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing page session"))?
            .to_owned();
        if hidden {
            // A target without a native window starts with a zero-size viewport.
            // Size only its render surface; retain the real browser identity.
            self.call(
                "Emulation.setDeviceMetricsOverride",
                json!({
                    "width":1280,"height":900,"deviceScaleFactor":1,"mobile":false
                }),
                Some(&session),
            )
            .map_err(|_| io::Error::other("E_HIDDEN_VIEWPORT"))?;
            // Hidden targets have no OS focus owner. Give their render surface
            // focus for normal DOM editing and keyboard menu interactions.
            self.call(
                "Emulation.setFocusEmulationEnabled",
                json!({"enabled":true}),
                Some(&session),
            )
            .map_err(|_| io::Error::other("E_HIDDEN_VIEWPORT"))?;
        }
        Ok(ManagedPage {
            target,
            session,
            fixture,
            hidden,
        })
    }

    fn page_origin(&mut self, page: &ManagedPage) -> io::Result<()> {
        let result = self.call("Page.getFrameTree", json!({}), Some(&page.session))?;
        let url = result["frameTree"]["frame"]["url"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing page origin"))?;
        if (page.fixture && url == "about:blank")
            || (!page.fixture && url.starts_with("https://chatgpt.com/"))
        {
            return Ok(());
        }
        Err(io::Error::other("E_OFFICIAL_ORIGIN_REQUIRED"))
    }

    fn dom(
        &mut self,
        page: &ManagedPage,
        function: &'static str,
        arguments: Vec<Value>,
    ) -> io::Result<Value> {
        self.page_origin(page)?;
        let root = self.call(
            "Runtime.evaluate",
            json!({"expression":"globalThis","returnByValue":false}),
            Some(&page.session),
        )?;
        let object = root["result"]["objectId"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing page global"))?;
        // All functions are reviewed bundled sources, never IPC/model data.
        let effort_label = include_str!("dom/effort_label.js");
        let model_families = include_str!("dom/model_families.js");
        let guarded = format!(
            "function(expectedOrigin, args) {{ if (location.origin !== expectedOrigin || (expectedOrigin === 'null' && location.href !== 'about:blank')) throw new Error('E_OFFICIAL_ORIGIN_REQUIRED'); const readEffortLabel = ({effort_label}); const readModelFamilies = ({model_families}); return ({function})(...args); }}"
        );
        let result = self.call("Runtime.callFunctionOn", json!({"objectId":object,"functionDeclaration":guarded,"arguments":[{"value":if page.fixture {"null"} else {"https://chatgpt.com"}},{"value":arguments}],"returnByValue":true}), Some(&page.session));
        let _ = self.call(
            "Runtime.releaseObject",
            json!({"objectId":object}),
            Some(&page.session),
        );
        let result = result?;
        if result.get("exceptionDetails").is_some() {
            let first_line = result["exceptionDetails"]["exception"]["description"]
                .as_str()
                .and_then(|text| text.lines().next());
            for code in [
                "E_MODEL_FAMILY",
                "E_MODEL_EFFORT_LABEL",
                "E_MODEL_MENU",
                "E_MODEL_SLIDER",
                "E_MODEL_FOCUS",
            ] {
                if first_line == Some(format!("Error: {code}").as_str()) {
                    return Err(io::Error::other(code));
                }
            }
            return Err(io::Error::other("E_BROWSER_ADAPTER"));
        }
        result["result"]
            .get("value")
            .cloned()
            .ok_or_else(|| io::Error::other("missing DOM result"))
    }

    pub fn baseline(&mut self, page: &ManagedPage) -> io::Result<Baseline> {
        serde_json::from_value(self.dom(page, include_str!("dom/baseline.js"), vec![])?)
            .map_err(|_| io::Error::other("E_BROWSER_ADAPTER"))
    }

    /// Non-generative structural evidence only. A candidate is not a certified
    /// account identity; no password fields, cookies, tokens or email are read.
    pub fn login_observation(&mut self, page: &ManagedPage) -> io::Result<LoginObservation> {
        let frame = self.call("Page.getFrameTree", json!({}), Some(&page.session))?;
        if !frame["frameTree"]["frame"]["url"]
            .as_str()
            .is_some_and(|url| url.starts_with("https://chatgpt.com/"))
        {
            return Ok(LoginObservation {
                verification_required: false,
                document_ready: false,
                browser_language: None,
                page_language: None,
                official_page: false,
                composer: false,
                account_surface: false,
                login_action: false,
                selected_label: None,
            });
        }
        serde_json::from_value(self.dom(page, include_str!("dom/login.js"), vec![])?)
            .map_err(|_| io::Error::other("E_BROWSER_ADAPTER"))
    }

    /// Opens the ordinary model menu, observes its currently rendered choices,
    /// then closes it. This is non-generative and never reads cookies or auth data.
    pub fn discover_models(&mut self, page: &ManagedPage) -> io::Result<ModelSurface> {
        let mut surface = self.discover_family_routes(page)?;
        let original = surface
            .candidates
            .iter()
            .find(|candidate| candidate.selected)
            .ok_or_else(|| io::Error::other("E_MODEL_SELECTION"))?
            .clone();
        let families = surface.diagnostic.families.clone();
        let scanned = (|| {
            for family in families
                .iter()
                .filter(|family| !family.selected && !family.disabled)
            {
                self.select_family(page, &family.identity)?;
                let mut next = self.discover_family_routes(page)?;
                if !next
                    .diagnostic
                    .families
                    .iter()
                    .any(|observed| observed.selected && observed.identity == family.identity)
                {
                    return Err(io::Error::other("E_MODEL_FAMILY"));
                }
                for candidate in &mut next.candidates {
                    candidate.selected = false;
                }
                surface.candidates.extend(next.candidates);
                if surface.candidates.len() > 64 {
                    return Err(io::Error::other("E_MODEL_RESULT"));
                }
            }
            Ok(())
        })();
        if !self
            .select_candidate(page, &original.identity)
            .is_ok_and(|label| label == original.label)
        {
            return Err(io::Error::other("E_MODEL_RESTORE"));
        }
        scanned?;
        Ok(surface)
    }

    fn discover_family_routes(&mut self, page: &ManagedPage) -> io::Result<ModelSurface> {
        self.model_diagnostic = None;
        if !page.fixture {
            let login = self.login_observation(page)?;
            if !login.official_page
                || !login.composer
                || !login.account_surface
                || login.login_action
                || login.verification_required
            {
                return Err(io::Error::other("E_LOGIN_REQUIRED"));
            }
        }
        let baseline = self.baseline(page)?;
        if !baseline.composer_empty || baseline.generating {
            return Err(io::Error::other("E_BROWSER_BUSY"));
        }
        self.open_model_menu(page)
            .map_err(|_| io::Error::other("E_MODEL_OPEN"))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let observed = loop {
            let observed = self
                .dom(page, include_str!("dom/model_surface.js"), vec![])
                .map_err(|_| io::Error::other("E_MODEL_READ"));
            match &observed {
                Ok(value)
                    if value["candidates"]
                        .as_array()
                        .is_some_and(|items| !items.is_empty()) =>
                {
                    break observed;
                }
                _ if Instant::now() >= deadline => break observed,
                _ => std::thread::sleep(Duration::from_millis(200)),
            }
        };
        let closed = self
            .close_model_menu(page)
            .map_err(|_| io::Error::other("E_MODEL_CLOSE"));
        let value = observed?;
        closed?;
        let mut surface: ModelSurface =
            serde_json::from_value(value).map_err(|_| io::Error::other("E_MODEL_PARSE"))?;
        self.model_diagnostic = Some(surface.diagnostic.clone());
        if let Some(range) = &surface.diagnostic.effort_range {
            let family = surface
                .diagnostic
                .families
                .iter()
                .find(|family| family.selected)
                .ok_or_else(|| io::Error::other("E_MODEL_FAMILY"))?;
            let original = surface
                .candidates
                .first()
                .filter(|candidate| surface.candidates.len() == 1 && candidate.selected)
                .ok_or_else(|| io::Error::other("E_MODEL_RESULT"))?
                .clone();
            if range.min > range.current
                || range.current > range.max
                || range
                    .max
                    .checked_sub(range.min)
                    .is_none_or(|width| width >= 5)
            {
                return Err(io::Error::other("E_MODEL_RESULT"));
            }
            // Observe every actual label instead of fabricating names from positions.
            // Restore the starting selection even if a later position cannot be read.
            let scanned = (|| {
                let mut candidates = Vec::new();
                for value in range.min..=range.max {
                    let identity = serde_json::to_string(&(
                        "reasoning-slider-v2",
                        &family.identity,
                        range.min,
                        range.max,
                        value,
                    ))?;
                    let selected = value == range.current;
                    let label = if selected {
                        original.label.clone()
                    } else {
                        self.select_candidate(page, &identity)?
                    };
                    candidates.push(ModelCandidate {
                        label,
                        identity,
                        selected,
                    });
                }
                Ok::<_, io::Error>(candidates)
            })();
            let restored = self.select_candidate(page, &original.identity);
            if !restored.is_ok_and(|label| label == original.label) {
                return Err(io::Error::other("E_MODEL_RESTORE"));
            }
            surface.candidates = scanned?;
        }
        if surface.candidates.len() > 64
            || surface.candidates.iter().any(|candidate| {
                candidate.label.is_empty()
                    || candidate.label.len() > 120
                    || candidate.identity.is_empty()
                    || candidate.identity.len() > 240
                    || candidate.label.chars().any(char::is_control)
                    || candidate.identity.chars().any(char::is_control)
            })
            || surface.diagnostic.model_testids.len() > 32
            || surface.diagnostic.visible_roles.len() > 32
            || surface.diagnostic.composer_controls.len() > 8
            || surface
                .diagnostic
                .model_testids
                .iter()
                .chain(&surface.diagnostic.visible_roles)
                .chain(&surface.diagnostic.composer_controls)
                .any(|value| {
                    value.is_empty() || value.len() > 120 || value.chars().any(char::is_control)
                })
        {
            return Err(io::Error::other("E_MODEL_RESULT"));
        }
        Ok(surface)
    }

    /// Selects only a previously observed reasoning-slider identity and verifies
    /// the resulting accessible value. It cannot click an arbitrary DOM node.
    pub fn select_candidate(&mut self, page: &ManagedPage, identity: &str) -> io::Result<String> {
        if identity.len() > 240 || identity.chars().any(char::is_control) {
            return Err(io::Error::other("E_MODEL_IDENTITY"));
        }
        let (kind, family, min, max, target): (String, String, i64, i64, i64) =
            serde_json::from_str(identity).map_err(|_| io::Error::other("E_MODEL_IDENTITY"))?;
        if kind != "reasoning-slider-v2"
            || family.is_empty()
            || family.len() > 80
            || min > target
            || target > max
            || max.checked_sub(min).is_none_or(|width| width >= 5)
        {
            return Err(io::Error::other("E_MODEL_IDENTITY"));
        }
        self.select_family(page, &family)?;
        self.open_model_menu(page)
            .map_err(|_| io::Error::other("E_MODEL_OPEN"))?;
        let selected = (|| {
            let mut previous = None;
            for step in 0..5 {
                let deadline = Instant::now() + Duration::from_secs(5);
                let state = loop {
                    if let Ok(value) = self.dom(
                        page,
                        include_str!("dom/effort_state.js"),
                        vec![json!(identity)],
                    ) && value["current"].as_i64().is_some_and(|current| {
                        Some(current) != previous
                            && (value["target"].as_i64() != Some(current)
                                || value["candidate_label"].as_str().is_some())
                    }) {
                        break value;
                    }
                    if Instant::now() >= deadline {
                        return Err(io::Error::other("E_MODEL_SELECT"));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                };
                let current = state["current"]
                    .as_i64()
                    .ok_or_else(|| io::Error::other("E_MODEL_SELECT"))?;
                let target = state["target"]
                    .as_i64()
                    .ok_or_else(|| io::Error::other("E_MODEL_SELECT"))?;
                if current == target {
                    return state["candidate_label"]
                        .as_str()
                        .filter(|label| !label.is_empty() && label.len() <= 120)
                        .map(str::to_owned)
                        .ok_or_else(|| io::Error::other("E_MODEL_SELECT"));
                }
                if step == 4 {
                    return Err(io::Error::other("E_MODEL_SELECT"));
                }
                let key = if target > current {
                    "ArrowRight"
                } else {
                    "ArrowLeft"
                };
                previous = Some(current);
                for event_type in ["rawKeyDown", "keyUp"] {
                    self.call(
                        "Input.dispatchKeyEvent",
                        json!({"type":event_type,"key":key,"code":key,
                            "windowsVirtualKeyCode":if target > current {39} else {37}}),
                        Some(&page.session),
                    )?;
                }
            }
            Err(io::Error::other("E_MODEL_SELECT"))
        })();
        let closed = self
            .close_model_menu(page)
            .map_err(|_| io::Error::other("E_MODEL_CLOSE"));
        let selected = selected?;
        closed?;
        Ok(selected)
    }

    fn select_family(&mut self, page: &ManagedPage, identity: &str) -> io::Result<()> {
        self.open_model_menu(page)
            .map_err(|_| io::Error::other("E_MODEL_OPEN"))?;
        let selected = (|| {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut opened = false;
            let mut clicked = false;
            loop {
                let state = self
                    .dom(
                        page,
                        include_str!("dom/select_family.js"),
                        vec![json!(identity), json!(opened), json!(clicked)],
                    )
                    .unwrap_or(Value::Null);
                if state["selected"] == true {
                    return Ok(());
                }
                if state["x"].is_number() && state["y"].is_number() {
                    self.click_point(page, &state)?;
                    match state["action"].as_str() {
                        Some("open") => opened = true,
                        Some("select") => clicked = true,
                        _ => return Err(io::Error::other("E_MODEL_FAMILY")),
                    }
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::other("E_MODEL_FAMILY"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })();
        let closed = self.close_model_menu(page);
        selected?;
        closed
    }

    /// Checks the complete family/effort route without changing it. This may be
    /// used with a populated composer immediately before the submission intent.
    pub fn verify_candidate(
        &mut self,
        page: &ManagedPage,
        identity: &str,
        label: &str,
    ) -> io::Result<()> {
        self.qualification_diagnostic = Some(QualificationDiagnostic {
            expected_model_label: label.to_owned(),
            observed_model_label: String::new(),
            user_present: false,
            user_matches: false,
            assistant_present: false,
            generating: false,
        });
        self.open_model_menu(page)?;
        let verified = (|| {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut last_failure = "E_MODEL_SELECTION";
            loop {
                let observation = self.dom(page, include_str!("dom/model_surface.js"), vec![]);
                if let Err(error) = &observation {
                    last_failure = match error.to_string().as_str() {
                        "E_MODEL_FAMILY" => "E_MODEL_FAMILY",
                        "E_MODEL_EFFORT_LABEL" => "E_MODEL_EFFORT_LABEL",
                        "E_MODEL_MENU" => "E_MODEL_MENU",
                        _ => "E_MODEL_READ",
                    };
                }
                if let Ok(value) = observation
                    && let Ok(surface) = serde_json::from_value::<ModelSurface>(value)
                {
                    self.model_diagnostic = Some(surface.diagnostic.clone());
                    let selected = surface
                        .candidates
                        .iter()
                        .filter(|item| item.selected)
                        .collect::<Vec<_>>();
                    if selected.len() == 1 {
                        self.qualification_diagnostic
                            .as_mut()
                            .unwrap()
                            .observed_model_label = selected[0].label.clone();
                        return if selected[0].identity == identity && selected[0].label == label {
                            Ok(())
                        } else {
                            Err(io::Error::other("E_MODEL_SELECTION"))
                        };
                    }
                }
                if Instant::now() >= deadline {
                    if let Some(diagnostic) = self.model_diagnostic.as_mut() {
                        diagnostic.read_failure = Some(last_failure.into());
                    }
                    return Err(io::Error::other("E_MODEL_SELECTION"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })();
        let closed = self.close_model_menu(page);
        verified?;
        closed
    }

    /// Opens an owned, isolated tab at ChatGPT's explicit Temporary Chat URL,
    /// verifies its URL and authenticated composer, then closes only that tab.
    pub fn verify_temporary_chat(&mut self) -> io::Result<bool> {
        match self.open_temporary_chat() {
            Ok(page) => {
                self.close_page(page)?;
                Ok(true)
            }
            Err(error) if error.to_string() == "E_TEMPORARY_CHAT" => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Runs one explicitly requested, non-retried qualification turn in a new
    /// Temporary Chat target. Retain at most one failed target for inspection.
    pub fn qualify_candidate(
        &mut self,
        identity: &str,
        expected_label: &str,
        prompt: &str,
        before_send: impl FnOnce() -> io::Result<()>,
        verify_scope: impl FnMut(&mut Self, &ManagedPage) -> io::Result<()>,
    ) -> io::Result<QualificationOutcome> {
        self.qualification_diagnostic = None;
        if let Some(previous) = self.failed_qualification.take() {
            let _ = self.close_page(previous);
        }
        let page = self.open_temporary_chat()?;
        let result = self.qualify_page(
            &page,
            identity,
            expected_label,
            prompt,
            before_send,
            verify_scope,
        );
        if result.is_err() {
            let _ = self.stop(&page);
            self.failed_qualification = Some(page);
            return result;
        }
        let closed = self.close_page(page);
        let outcome = result?;
        closed?;
        Ok(outcome)
    }

    pub fn qualification_diagnostic(&self) -> Option<QualificationDiagnostic> {
        self.qualification_diagnostic.clone()
    }

    pub fn model_diagnostic(&self) -> Option<ModelSurfaceDiagnostic> {
        self.model_diagnostic.clone()
    }

    pub fn account_scope(&mut self, page: &ManagedPage) -> io::Result<ScopeSurface> {
        self.prepare_page(page)?;
        let opened = self
            .dom(page, include_str!("dom/open_account.js"), vec![])
            .map_err(|_| io::Error::other("E_ACCOUNT_OPEN"))?;
        if opened["x"].is_number() && opened["y"].is_number() {
            self.click_point(page, &opened)
                .map_err(|_| io::Error::other("E_ACCOUNT_OPEN"))?;
        } else if opened != true {
            let failure = match opened["failure"].as_str() {
                Some("E_ACCOUNT_MISSING") => "E_ACCOUNT_MISSING",
                Some("E_ACCOUNT_AMBIGUOUS") => "E_ACCOUNT_AMBIGUOUS",
                _ => "E_ACCOUNT_OPEN",
            };
            let tags = opened["control_tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(8)
                .filter(|tag| {
                    tag.len() <= 32
                        && tag.chars().all(|c| {
                            c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, ':' | '-')
                        })
                })
                .map(str::to_owned)
                .collect();
            return Ok(ScopeSurface {
                account: None,
                workspace: None,
                default_workspace: false,
                diagnostic: ScopeDiagnostic {
                    failure: Some(failure.into()),
                    profile_control_tags: tags,
                    ..Default::default()
                },
            });
        }
        let result = (|| {
            let deadline = Instant::now() + Duration::from_secs(5);
            let result: io::Result<ScopeSurface> = loop {
                match self.dom(page, include_str!("dom/account_scope.js"), vec![]) {
                    Ok(value) if !value.is_null() => {
                        break serde_json::from_value(value)
                            .map_err(|_| io::Error::other("E_ACCOUNT_PARSE"));
                    }
                    Err(_) => break Err(io::Error::other("E_ACCOUNT_READ")),
                    _ if Instant::now() >= deadline => {
                        break Err(io::Error::other("E_ACCOUNT_SCOPE"));
                    }
                    _ => std::thread::sleep(Duration::from_millis(100)),
                }
            };
            let mut surface = result?;
            let mut switcher_account = None;
            let switcher_deadline = Instant::now() + Duration::from_secs(2);
            let mut hovered = false;
            loop {
                if let Ok(value) = self.dom(page, include_str!("dom/account_switcher.js"), vec![])
                    && !value.is_null()
                {
                    if !hovered
                        && value["point"]["x"].is_number()
                        && value["point"]["y"].is_number()
                    {
                        let (x, y) = Self::point_coordinates(&value["point"])?;
                        self.call(
                            "Input.dispatchMouseEvent",
                            json!({"type":"mouseMoved","x":x,"y":y,"button":"none","buttons":0}),
                            Some(&page.session),
                        )?;
                        hovered = true;
                    }
                    surface.diagnostic.switcher =
                        serde_json::from_value(value["diagnostic"].clone()).ok();
                    switcher_account = value["account"]
                        .as_str()
                        .filter(|account| account.len() <= 320)
                        .map(str::to_owned);
                    if value["diagnostic"]["submenu_present"] == true {
                        break;
                    }
                }
                if Instant::now() >= switcher_deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if surface
                .diagnostic
                .switcher
                .as_ref()
                .is_some_and(|switcher| switcher.submenu_present)
            {
                // Inspecting the submenu never selects an account or workspace.
                // Leave its hover area, dismiss it, then restore the parent menu.
                self.call(
                    "Input.dispatchMouseEvent",
                    json!({"type":"mouseMoved","x":10,"y":10,"button":"none","buttons":0}),
                    Some(&page.session),
                )?;
                for event_type in ["rawKeyDown", "keyUp"] {
                    self.call("Input.dispatchKeyEvent", json!({"type":event_type,"key":"Escape","code":"Escape","windowsVirtualKeyCode":27}), Some(&page.session))?;
                }
                let opened = self.dom(page, include_str!("dom/open_account.js"), vec![])?;
                if opened["x"].is_number() && opened["y"].is_number() {
                    self.click_point(page, &opened)?;
                }
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if self
                        .dom(page, include_str!("dom/account_scope.js"), vec![])
                        .is_ok_and(|value| !value.is_null())
                    {
                        break;
                    }
                    if Instant::now() >= deadline {
                        return Err(io::Error::other("E_ACCOUNT_OPEN"));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            if surface.account.is_none() && surface.diagnostic.settings_available {
                let settings_opened =
                    self.dom(page, include_str!("dom/open_account_settings.js"), vec![])? == true;
                surface.diagnostic.settings_opened = settings_opened;
                if settings_opened {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    let mut navigated = false;
                    loop {
                        if let Ok(value) =
                            self.dom(page, include_str!("dom/account_settings.js"), vec![])
                            && !value.is_null()
                        {
                            surface.diagnostic.settings_controls = value["controls"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .take(16)
                                .map(str::to_owned)
                                .collect();
                            if !navigated
                                && surface
                                    .diagnostic
                                    .settings_controls
                                    .iter()
                                    .any(|label| label == "Account")
                            {
                                let point = self.dom(
                                    page,
                                    include_str!("dom/open_account_tab.js"),
                                    vec![],
                                )?;
                                if point["selected"] == true {
                                    navigated = true;
                                } else if point["focused"] == true {
                                    for event_type in ["keyDown", "keyUp"] {
                                        self.call("Input.dispatchKeyEvent", json!({"type":event_type,"key":"Enter","code":"Enter","text":if event_type == "keyDown" {"\r"} else {""},"unmodifiedText":if event_type == "keyDown" {"\r"} else {""},"windowsVirtualKeyCode":13,"nativeVirtualKeyCode":13}), Some(&page.session))?;
                                    }
                                    navigated = true;
                                }
                            }
                            surface.diagnostic.settings_account_selected =
                                value["account_selected"] == true;
                            surface.diagnostic.settings_panel_present =
                                value["panel_present"] == true;
                            surface.diagnostic.settings_fields = value["fields"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .take(16)
                                .map(str::to_owned)
                                .collect();
                            surface.diagnostic.settings_loading = value["panel_loading"] == true;
                            surface.diagnostic.settings_account_candidates =
                                value["account_candidates"].as_u64().unwrap_or(0) as usize;
                            if value["account_selected"] == true
                                && value["account_candidates"].as_u64().unwrap_or(0) > 0
                            {
                                surface.account = value["account"]
                                    .as_str()
                                    .filter(|account| account.len() <= 320)
                                    .map(str::to_owned);
                                break;
                            }
                        }
                        if Instant::now() >= deadline {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
            surface.diagnostic.account_switcher_matches_settings =
                surface.diagnostic.settings_account_candidates == 1
                    && switcher_account.is_some()
                    && surface.account == switcher_account;
            surface.default_workspace = qualifies_default_context(&surface);
            surface.diagnostic.default_context_verified = surface.default_workspace;
            Ok(surface)
        })();
        let closed = self.close_model_menu(page);
        closed?;
        {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.dom(page, include_str!("dom/settings_closed.js"), vec![])? != true {
                if Instant::now() >= deadline {
                    return Err(io::Error::other("E_ACCOUNT_SETTINGS_CLOSE"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        result
    }

    fn qualify_page(
        &mut self,
        page: &ManagedPage,
        identity: &str,
        expected_label: &str,
        prompt: &str,
        before_send: impl FnOnce() -> io::Result<()>,
        mut verify_scope: impl FnMut(&mut Self, &ManagedPage) -> io::Result<()>,
    ) -> io::Result<QualificationOutcome> {
        verify_scope(self, page)?;
        let candidate_label = self
            .select_candidate(page, identity)
            .map_err(|_| io::Error::other("E_QUALIFICATION_SELECT"))?;
        if candidate_label != expected_label {
            return Err(io::Error::other("E_MODEL_SELECTION"));
        }
        let baseline = self
            .baseline(page)
            .map_err(|_| io::Error::other("E_QUALIFICATION_BASELINE"))?;
        let mut tracker = TurnTracker::new(baseline.clone(), &baseline.selected_model)
            .map_err(io::Error::other)?;
        self.insert_prompt(page, prompt)
            .map_err(|_| io::Error::other("E_QUALIFICATION_INSERT"))?;
        self.verify_candidate(page, identity, expected_label)?;
        before_send()?;
        tracker.begin_submission().map_err(io::Error::other)?;
        if let Err(error) = self.press_send(page, prompt, &baseline.selected_model) {
            let code = match error.to_string().as_str() {
                "E_SEND_SURFACE" => "E_SEND_SURFACE",
                "E_SEND_DISABLED" => "E_SEND_DISABLED",
                "E_MODEL_SELECTION" => "E_MODEL_SELECTION",
                "E_COMPOSER_MISMATCH" => "E_COMPOSER_MISMATCH",
                _ => "E_SUBMISSION_UNCERTAIN",
            };
            return Err(io::Error::other(code));
        }
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            let observation = self
                .observe(page, &baseline, prompt)
                .map_err(|_| io::Error::other("E_QUALIFICATION_OBSERVE"))?;
            self.qualification_diagnostic = Some(QualificationDiagnostic {
                expected_model_label: baseline.selected_model.clone(),
                observed_model_label: observation.selected_model.clone(),
                user_present: observation.user_id.is_some(),
                user_matches: observation.user_matches,
                assistant_present: observation.assistant_id.is_some(),
                generating: observation.generating,
            });
            match tracker.observe(observation).map_err(io::Error::other)? {
                Progress::Completed(response) => {
                    verify_scope(self, page)?;
                    return Ok(QualificationOutcome {
                        candidate_label,
                        response,
                    });
                }
                Progress::AwaitingAcknowledgement | Progress::Generating => {}
            }
            if Instant::now() >= deadline {
                let _ = self.stop(page);
                return Err(io::Error::other("E_QUALIFICATION_TIMEOUT"));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    pub fn open_temporary_chat(&mut self) -> io::Result<ManagedPage> {
        let page = self.open_hidden_page("https://chatgpt.com/?temporary-chat=true", false)?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if self
                .dom(&page, include_str!("dom/temporary_chat.js"), vec![])
                .is_ok_and(|value| value == true)
            {
                return Ok(page);
            }
            if Instant::now() >= deadline {
                self.close_page(page)?;
                return Err(io::Error::other("E_TEMPORARY_CHAT"));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn open_model_menu(&mut self, page: &ManagedPage) -> io::Result<()> {
        let result = self.open_model_menu_inner(page);
        if result.is_err()
            && let Ok(value) = self.dom(page, include_str!("dom/model_open_state.js"), vec![])
            && let Ok(state) =
                serde_json::from_value::<std::collections::BTreeMap<String, bool>>(value)
        {
            self.model_diagnostic
                .get_or_insert_with(Default::default)
                .interaction_state
                .extend(state);
        }
        result
    }

    fn open_model_menu_inner(&mut self, page: &ManagedPage) -> io::Result<()> {
        self.prepare_page(page)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let point = loop {
            if let Ok(point) = self.dom(page, include_str!("dom/focus_models.js"), vec![]) {
                break point;
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("E_MODEL_MENU"));
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        if point["expanded"] == true {
            return Ok(());
        }
        for event_type in ["rawKeyDown", "keyUp"] {
            self.call("Input.dispatchKeyEvent", json!({"type":event_type,"key":"Enter","code":"Enter","windowsVirtualKeyCode":13,"nativeVirtualKeyCode":13}), Some(&page.session))?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self
                .dom(page, include_str!("dom/focus_models.js"), vec![])
                .is_ok_and(|value| value["expanded"] == true)
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("E_MODEL_OPEN"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn prepare_page(&mut self, page: &ManagedPage) -> io::Result<()> {
        if !page.hidden {
            self.call("Page.bringToFront", json!({}), Some(&page.session))?;
        }
        Ok(())
    }

    fn point_coordinates(point: &Value) -> io::Result<(f64, f64)> {
        let (Some(x), Some(y)) = (point["x"].as_f64(), point["y"].as_f64()) else {
            return Err(io::Error::other("E_MODEL_MENU"));
        };
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..=10_000.0).contains(&x)
            || !(0.0..=10_000.0).contains(&y)
        {
            return Err(io::Error::other("E_MODEL_MENU"));
        }
        Ok((x, y))
    }

    fn click_point(&mut self, page: &ManagedPage, point: &Value) -> io::Result<()> {
        let (x, y) = Self::point_coordinates(point)?;
        for (event_type, button, buttons) in [
            ("mouseMoved", "none", 0),
            ("mousePressed", "left", 1),
            ("mouseReleased", "left", 0),
        ] {
            self.call(
                "Input.dispatchMouseEvent",
                json!({
                    "type":event_type,"x":x,"y":y,"button":button,"buttons":buttons,"clickCount":1
                }),
                Some(&page.session),
            )?;
        }
        Ok(())
    }

    fn close_model_menu(&mut self, page: &ManagedPage) -> io::Result<()> {
        for event_type in ["rawKeyDown", "keyUp"] {
            self.call(
                "Input.dispatchKeyEvent",
                json!({
                    "type":event_type,
                    "key":"Escape",
                    "code":"Escape",
                    "windowsVirtualKeyCode":27,
                    "nativeVirtualKeyCode":27
                }),
                Some(&page.session),
            )?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.dom(page, include_str!("dom/menus_closed.js"), vec![])? == true {
                return Ok(());
            }
            if Instant::now() >= deadline {
                if let Ok(value) = self.dom(page, include_str!("dom/model_surface.js"), vec![]) {
                    self.model_diagnostic =
                        serde_json::from_value(value["diagnostic"].clone()).ok();
                }
                return Err(io::Error::other("E_MODEL_CLOSE"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn insert_prompt(&mut self, page: &ManagedPage, prompt: &str) -> io::Result<()> {
        if prompt.len() > 512 * 1024 {
            return Err(io::Error::other("E_CONTEXT_BUDGET"));
        }
        if self.dom(page, include_str!("dom/focus.js"), vec![])? != true {
            return Err(io::Error::other("E_COMPOSER_FOCUS"));
        }
        // Keep literal input out of the editor's live-typing shortcuts. The
        // prompt remains a CDP argument, never executable JavaScript.
        if self.dom(
            page,
            include_str!("dom/insert_prompt.js"),
            vec![json!(prompt)],
        )? != true
        {
            return Err(io::Error::other("E_COMPOSER_INSERT"));
        }
        Ok(())
    }

    /// Exactly one click; callers must persist submitting state before this call.
    /// A timeout is uncertain and must never cause an automatic retry.
    pub fn press_send(
        &mut self,
        page: &ManagedPage,
        prompt: &str,
        selected_model: &str,
    ) -> io::Result<()> {
        let outcome = self.dom(
            page,
            include_str!("dom/send.js"),
            vec![json!(prompt), json!(selected_model)],
        )?;
        match outcome {
            Value::Bool(true) => Ok(()),
            Value::String(code)
                if matches!(
                    code.as_str(),
                    "E_SEND_SURFACE"
                        | "E_SEND_DISABLED"
                        | "E_MODEL_SELECTION"
                        | "E_COMPOSER_MISMATCH"
                ) =>
            {
                Err(io::Error::other(code))
            }
            _ => Err(io::Error::other("E_SUBMISSION_UNCERTAIN")),
        }
    }

    pub fn observe(
        &mut self,
        page: &ManagedPage,
        baseline: &Baseline,
        prompt: &str,
    ) -> io::Result<Observation> {
        serde_json::from_value(self.dom(
            page,
            include_str!("dom/observe.js"),
            vec![json!(baseline.ids), json!(prompt)],
        )?)
        .map_err(|_| io::Error::other("E_BROWSER_ADAPTER"))
    }

    pub fn stop(&mut self, page: &ManagedPage) -> io::Result<bool> {
        Ok(self.dom(page, include_str!("dom/stop.js"), vec![])? == true)
    }

    pub fn close_page(&mut self, page: ManagedPage) -> io::Result<()> {
        self.close_page_checked(&page)
    }

    /// Keep the page lease until closure is confirmed, so cleanup can be retried.
    pub fn close_page_checked(&mut self, page: &ManagedPage) -> io::Result<()> {
        let _ = self.call("Target.closeTarget", json!({"targetId":page.target}), None);
        // A user may already have closed the owned tab. Confirm absence rather
        // than claiming cleanup from an error or the deprecated `success`
        // acknowledgement, which can arrive before target destruction.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if !self.page_exists(page)? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("E_BROWSER_RELEASE"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Development qualification only. Uses bundled synthetic markup in a fresh
    /// blank target. It cannot qualify selectors against a real account.
    pub fn probe_dom(&mut self) -> io::Result<Value> {
        let background = self.headless || self.offscreen;
        let mut result = self.probe_dom_mode(background)?;
        if background {
            let second = self.probe_dom_mode(false)?;
            for field in ["initial_desktop_exposure", "initial_taskbar_exposure"] {
                if self.offscreen {
                    result[field] = json!(result[field] == true || second[field] == true);
                }
            }
        }
        self.probe_startup_page()?;
        result["background_page_fixture"] = json!(if background { "PASS" } else { "NOT RUN" });
        result["headless"] = json!(self.headless);
        result["offscreen"] = json!(self.offscreen);
        result["owned_targets_released"] = json!(true);
        result["css_animation_completed"] = json!(true);
        Ok(result)
    }

    fn probe_dom_mode(&mut self, hidden: bool) -> io::Result<Value> {
        let params = if self.offscreen {
            let (x, y, width, height) = BrowserProcess::background_bounds();
            json!({"url":"about:blank", "newWindow":true, "background":true, "left":x,"top":y,"width":width,"height":height})
        } else {
            json!({"url":"about:blank", "hidden":hidden && !self.headless, "background":hidden})
        };
        let result = self.call("Target.createTarget", params, None)?;
        let target = result["targetId"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing fixture target"))?
            .to_owned();
        let native_initial = if self.offscreen {
            Some(self.process.park_windows()?)
        } else {
            None
        };
        let page = self.attach(target, true, hidden || self.offscreen)?;
        let result = self.run_dom_fixture(&page);
        let result = result.map_err(|error| {
            let metrics = self.dom(&page, "function () { return { width:innerWidth, height:innerHeight, focused:document.hasFocus(), visible:document.visibilityState === 'visible' }; }", vec![]).unwrap_or(Value::Null);
            io::Error::other(format!("E_DOM_FIXTURE hidden={hidden} metrics={metrics}: {error}"))
        });
        let closed = self.close_page(page);
        let mut result = result?;
        closed?;
        if let Some((windows, exposed, taskbar)) = native_initial {
            result["native_window_count"] = json!(windows);
            result["initial_desktop_exposure"] = json!(exposed);
            result["initial_taskbar_exposure"] = json!(taskbar);
        }
        Ok(result)
    }

    fn run_dom_fixture(&mut self, page: &ManagedPage) -> io::Result<Value> {
        let frame = self.call("Page.getFrameTree", json!({}), Some(&page.session))?;
        for wrong_label in [true, false] {
            self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture.html")}), Some(&page.session))?;
            let animation_deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if self.dom(page, "function () { const animations = document.getElementById('fixture-animation').getAnimations(); return animations.length === 1 && animations[0].playState === 'finished'; }", vec![])? == true {
                    break;
                }
                if Instant::now() >= animation_deadline {
                    return Err(io::Error::other("E_BACKGROUND_ANIMATION"));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let label = if wrong_label {
                "Unobserved route"
            } else {
                "Fixture text mode · Standard"
            };
            let mut intent_called = false;
            let attempt = self.qualify_page(
                page,
                r#"["reasoning-slider-v2","Fixture text mode",0,1,0]"#,
                label,
                "Never submit this fixture",
                || {
                    intent_called = true;
                    Err(io::Error::other("E_FIXTURE_DURABILITY_FAILURE"))
                },
                |_, _| Ok(()),
            );
            if attempt.is_ok()
                || intent_called == wrong_label
                || self.baseline(page)?.ids != ["old-assistant"]
            {
                return Err(io::Error::other(format!(
                    "E_QUALIFICATION_GUARD_FIXTURE: {}",
                    attempt
                        .err()
                        .map_or_else(|| "unexpected success".into(), |error| error.to_string())
                )));
            }
        }
        self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture.html")}), Some(&page.session))?;
        let mut intents = 0;
        let mut scope_checks = 0;
        let qualified = self.qualify_page(
            page,
            r#"["reasoning-slider-v2","Fixture text mode",0,1,1]"#,
            "Fixture text mode · Extended",
            "Fixture qualification",
            || {
                intents += 1;
                Ok(())
            },
            |_, _| {
                scope_checks += 1;
                Ok(())
            },
        )?;
        if intents != 1 || scope_checks != 2 || qualified.response != "fixture response" {
            return Err(io::Error::other("E_QUALIFICATION_FIXTURE"));
        }
        // A changed account before submission must send nothing; a change
        // detected after completion must withhold the response, never resend it.
        for fail_on_check in [1, 2] {
            self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture.html")}), Some(&page.session))?;
            let mut intents = 0;
            let mut checks = 0;
            let result = self.qualify_page(
                page,
                r#"["reasoning-slider-v2","Fixture text mode",0,1,0]"#,
                "Fixture text mode · Standard",
                "Scope change fixture",
                || {
                    intents += 1;
                    Ok(())
                },
                |_, _| {
                    checks += 1;
                    if checks == fail_on_check {
                        Err(io::Error::other("E_SESSION_SCOPE"))
                    } else {
                        Ok(())
                    }
                },
            );
            if result
                .err()
                .is_none_or(|error| error.to_string() != "E_SESSION_SCOPE")
                || checks != fail_on_check
                || intents != fail_on_check - 1
                || self.baseline(page)?.ids.len() != if fail_on_check == 1 { 1 } else { 3 }
            {
                return Err(io::Error::other("E_SCOPE_GUARD_FIXTURE"));
            }
        }
        self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture.html")}), Some(&page.session))?;
        let baseline = self
            .baseline(page)
            .map_err(|_| io::Error::other("E_FIXTURE_BASELINE"))?;
        if self
            .press_send(page, "different prompt", &baseline.selected_model)
            .is_ok()
            || self.baseline(page)?.ids != baseline.ids
        {
            return Err(io::Error::other("E_SEND_GUARD_FIXTURE"));
        }
        let surface = self
            .discover_models(page)
            .map_err(|_| io::Error::other("E_FIXTURE_DISCOVERY"))?;
        let scope = self.account_scope(page)?;
        if scope.account.as_deref() != Some("fixture@example.invalid")
            || scope.workspace.as_deref() != Some("fixture-workspace")
            || !scope.diagnostic.settings_opened
            || scope.diagnostic.settings_account_candidates != 1
            || !scope
                .diagnostic
                .switcher
                .as_ref()
                .is_some_and(|switcher| switcher.submenu_present && switcher.selected_items == 1)
        {
            return Err(io::Error::other(format!(
                "E_SCOPE_FIXTURE: {}",
                serde_json::to_string(&scope.diagnostic)?
            )));
        }
        if surface.candidates.len() != 4
            || !surface.temporary_chat
            || surface
                .candidates
                .iter()
                .filter(|route| route.selected)
                .count()
                != 1
        {
            return Err(io::Error::other("E_MODEL_FIXTURE"));
        }
        let selected = self
            .select_candidate(page, &surface.candidates[1].identity)
            .map_err(|_| io::Error::other("E_FIXTURE_SELECTION"))?;
        if selected != "Fixture text mode · Extended" {
            return Err(io::Error::other("E_MODEL_SELECTION_FIXTURE"));
        }
        let selected_surface = self
            .discover_models(page)
            .map_err(|_| io::Error::other("E_FIXTURE_SELECTED_DISCOVERY"))?;
        if selected_surface
            .candidates
            .iter()
            .find(|candidate| candidate.selected)
            .map(|candidate| candidate.label.as_str())
            != Some(selected.as_str())
        {
            return Err(io::Error::other("E_MODEL_SELECTION_FIXTURE"));
        }
        let mut tracker =
            TurnTracker::new(baseline.clone(), "Fixture text mode").map_err(io::Error::other)?;
        let prompt = "Literal input: quotes \" ' ` ${never_execute()} <script>throw 1</script>\nUnicode: 🦀 🦀";
        self.insert_prompt(page, prompt)
            .map_err(|_| io::Error::other("E_FIXTURE_INSERT"))?;
        tracker.begin_submission().map_err(io::Error::other)?;
        self.press_send(page, prompt, &baseline.selected_model)
            .map_err(|_| io::Error::other("E_FIXTURE_SEND"))?;
        let observation = self
            .observe(page, &baseline, prompt)
            .map_err(|_| io::Error::other("E_FIXTURE_OBSERVE"))?;
        // A collapsed message must match its full content, not its visible
        // prefix, and its expand-control label is never part of the prompt.
        for mismatch in ["Literal input:", &format!("{prompt}\nShow more")] {
            if self.observe(page, &baseline, mismatch)?.user_matches {
                return Err(io::Error::other("E_MESSAGE_MATCH_FIXTURE"));
            }
        }
        match tracker.observe(observation).map_err(io::Error::other)? {
            Progress::Completed(text) if text == "fixture response" => {
                self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture.html")}), Some(&page.session))?;
                self.insert_prompt(page, "Unsent fixture draft")?;
                if self
                    .verify_candidate(
                        page,
                        r#"["reasoning-slider-v2","Fixture reasoning mode",0,1,0]"#,
                        "Fixture reasoning mode · Standard",
                    )
                    .is_ok()
                {
                    return Err(io::Error::other("E_FAMILY_VERIFY_FIXTURE"));
                }
                self.verify_candidate(
                    page,
                    r#"["reasoning-slider-v2","Fixture text mode",0,1,0]"#,
                    "Fixture text mode · Standard",
                )?;
                if self.discover_models(page).err().map(|error| error.to_string()).as_deref() != Some("E_BROWSER_BUSY")
                    || self.dom(page, "function () { return document.querySelector('#prompt-textarea').textContent === 'Unsent fixture draft' && document.querySelector('[role=slider]').getAttribute('aria-valuenow') === '0'; }", vec![])? != true
                {
                    return Err(io::Error::other("E_DISCOVERY_BUSY_FIXTURE"));
                }
                self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture.html")}), Some(&page.session))?;
                self.dom(page, "function () { document.body.dataset.fixtureEffortFailure = 'Fixture reasoning mode'; return true; }", vec![])?;
                if self.discover_models(page).is_ok()
                    || self.dom(page, "function () { return document.querySelector('[role=slider]').getAttribute('aria-valuenow') === '0' && document.querySelector('#fixture-effort-label').textContent === 'Standard, 1 of 2.' && document.querySelector('[data-testid=model-fixture-text]').getAttribute('aria-checked') === 'true'; }", vec![])? != true
                    || self.baseline(page)?.ids != ["old-assistant"]
                {
                    return Err(io::Error::other("E_EFFORT_RESTORE_FIXTURE"));
                }
                self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture_stop.html")}), Some(&page.session))?;
                if !self.stop(page)? || self.baseline(page)?.generating || self.stop(page)? {
                    return Err(io::Error::other("E_CANCEL_FIXTURE"));
                }
                self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":"<!doctype html><title>Selector drift fixture</title>"}), Some(&page.session))?;
                if self.baseline(page).is_ok() {
                    return Err(io::Error::other("E_SELECTOR_DRIFT_FIXTURE"));
                }
                Ok(
                    json!({"result":"PASS","evidence":"synthetic DOM only","model_selection_verified":true,"family_selection_verified":true,"wrong_family_rejected":true,"effort_restore_on_failure":true,"stable_turn_identity_verified":true,"literal_prompt":true,"historical_message_excluded":true,"completion_attributed":true,"stop_control":true,"selector_drift_rejected":true}),
                )
            }
            _ => Err(io::Error::other("E_DOM_FIXTURE")),
        }
    }

    pub fn close(&mut self) -> io::Result<()> {
        // Browser.close may terminate the pipe before its reply is delivered.
        // Process exit, not a CDP acknowledgement, releases the profile lock.
        let _ = self.call("Browser.close", json!({}), None);
        self.process.wait_for_exit()
    }

    // Private primitive; callers cannot supply arbitrary JS through application IPC.
    fn call(
        &mut self,
        method: &'static str,
        params: Value,
        session: Option<&str>,
    ) -> io::Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let mut request = json!({"id":id,"method":method,"params":params});
        if let Some(session) = session {
            request["sessionId"] = json!(session);
        }
        let mut bytes = serde_json::to_vec(&request)?;
        if bytes.len() > MAX_FRAME {
            return Err(io::Error::other("CDP request too large"));
        }
        bytes.push(0);
        self.process.input.write_all(&bytes)?;
        self.process.input.flush()?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let response = self.replies.recv_timeout(remaining).map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "browser pipe unavailable")
            })??;
            if response["id"].as_u64() == Some(id) {
                if response.get("error").is_some() {
                    return Err(io::Error::other("browser operation rejected"));
                }
                return response
                    .get("result")
                    .cloned()
                    .ok_or_else(|| io::Error::other("missing browser reply"));
            }
        }
    }
}

/// Qualifies the observed account-switcher variant with no workspace selector.
/// This does not infer a provider-side personal workspace ID from missing data.
/// Any workspace control, organization marker or unknown menu layout invalidates
/// the UI-default scope and requires explicit workspace evidence instead.
fn qualifies_default_context(surface: &ScopeSurface) -> bool {
    let d = &surface.diagnostic;
    let Some(switcher) = &d.switcher else {
        return false;
    };
    let known_menu = d.menu_controls.len() == 6
        && matches!(
            d.menu_controls[0].as_str(),
            "menuitem:other:action" | "menuitem:other:submenu"
        )
        && d.menu_controls[1..]
            == [
                "menuitem:Personalization:action",
                "menuitem:other:action",
                "menuitem:Settings:action",
                "menuitem:Help:submenu",
                "menuitem:Log out:action",
            ];
    surface.account.is_some()
        && surface.workspace.is_none()
        && d.failure.is_none()
        && d.menu_present
        && known_menu
        && d.workspace_candidates == 0
        && d.selected_workspace_candidates == 0
        && d.selected_items == 0
        && !d.has_workspace_id
        && d.workspace_markers.len() == 1
        && matches!(d.workspace_markers[0].as_str(), "Pro" | "Plus" | "Free")
        && d.settings_opened
        && d.settings_account_selected
        && d.settings_panel_present
        && !d.settings_loading
        && d.settings_account_candidates == 1
        && d.account_switcher_matches_settings
        && switcher.available
        && switcher.expanded
        && switcher.submenu_present
        && switcher.account_candidates == 1
        && matches!(
            (
                switcher.account_source.as_deref(),
                switcher.selected_account_candidates
            ),
            (Some("selected_account"), 1) | (Some("account_heading"), 0)
        )
        && switcher.workspace_candidates == 0
        && switcher.selected_items == 1
        && switcher.public_labels == ["Add account"]
        && switcher.controls
            == [
                "menuitem:other:unselected",
                "menuitemradio:other:selected",
                "menuitem:Add account:unselected",
            ]
}

fn read_frame(input: &mut impl BufRead) -> io::Result<Vec<u8>> {
    let mut frame = Vec::new();
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "browser pipe closed",
            ));
        }
        let end = available.iter().position(|b| *b == 0);
        let count = end.unwrap_or(available.len());
        if frame.len() + count > MAX_FRAME {
            return Err(io::Error::other("CDP frame too large"));
        }
        frame.extend_from_slice(&available[..count]);
        input.consume(count + usize::from(end.is_some()));
        if end.is_some() {
            return Ok(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn default_scope() -> ScopeSurface {
        ScopeSurface {
            account: Some("fixture@example.invalid".into()),
            workspace: None,
            default_workspace: false,
            diagnostic: ScopeDiagnostic {
                menu_present: true,
                menu_controls: [
                    "menuitem:other:submenu",
                    "menuitem:Personalization:action",
                    "menuitem:other:action",
                    "menuitem:Settings:action",
                    "menuitem:Help:submenu",
                    "menuitem:Log out:action",
                ]
                .map(String::from)
                .into(),
                workspace_markers: vec!["Pro".into()],
                settings_opened: true,
                settings_account_selected: true,
                settings_panel_present: true,
                settings_account_candidates: 1,
                account_switcher_matches_settings: true,
                switcher: Some(AccountSwitcherDiagnostic {
                    account_source: Some("selected_account".into()),
                    selected_email_sources: vec![1, 1, 0],
                    email_controls: vec!["menuitemradio:selected".into()],
                    available: true,
                    expanded: true,
                    submenu_present: true,
                    account_candidates: 1,
                    workspace_candidates: 0,
                    selected_items: 1,
                    selected_account_candidates: 1,
                    public_labels: vec!["Add account".into()],
                    controls: [
                        "menuitem:other:unselected",
                        "menuitemradio:other:selected",
                        "menuitem:Add account:unselected",
                    ]
                    .map(String::from)
                    .into(),
                }),
                ..ScopeDiagnostic::default()
            },
        }
    }
    #[test]
    fn default_context_requires_corroborated_selected_account_and_known_layout() {
        assert!(qualifies_default_context(&default_scope()));
        let mut heading = default_scope();
        let switcher = heading.diagnostic.switcher.as_mut().unwrap();
        switcher.account_source = Some("account_heading".into());
        switcher.selected_account_candidates = 0;
        assert!(qualifies_default_context(&heading));
        heading.diagnostic.account_switcher_matches_settings = false;
        assert!(!qualifies_default_context(&heading));
        let mutations: &[fn(&mut ScopeSurface)] = &[
            |s| s.account = None,
            |s| s.workspace = Some("observed-workspace".into()),
            |s| s.diagnostic.account_switcher_matches_settings = false,
            |s| s.diagnostic.settings_account_candidates = 2,
            |s| s.diagnostic.settings_account_selected = false,
            |s| s.diagnostic.settings_loading = true,
            |s| s.diagnostic.settings_panel_present = false,
            |s| s.diagnostic.failure = Some("E_ACCOUNT_SCOPE".into()),
            |s| s.diagnostic.menu_controls[2] = "menuitem:Workspaces:submenu".into(),
            |s| s.diagnostic.workspace_markers = vec!["Business".into()],
            |s| s.diagnostic.workspace_markers.push("Enterprise".into()),
            |s| s.diagnostic.workspace_candidates = 1,
            |s| s.diagnostic.has_workspace_id = true,
            |s| s.diagnostic.switcher = None,
            |s| s.diagnostic.switcher.as_mut().unwrap().expanded = false,
            |s| s.diagnostic.switcher.as_mut().unwrap().account_candidates = 2,
            |s| {
                s.diagnostic
                    .switcher
                    .as_mut()
                    .unwrap()
                    .selected_account_candidates = 0
            },
            |s| s.diagnostic.switcher.as_mut().unwrap().workspace_candidates = 1,
            |s| s.diagnostic.switcher.as_mut().unwrap().selected_items = 2,
            |s| {
                s.diagnostic
                    .switcher
                    .as_mut()
                    .unwrap()
                    .public_labels
                    .push("Personal workspace".into())
            },
            |s| {
                s.diagnostic.switcher.as_mut().unwrap().controls[1] =
                    "menuitemradio:other:unselected".into()
            },
        ];
        for (index, mutate) in mutations.iter().enumerate() {
            let mut surface = default_scope();
            mutate(&mut surface);
            assert!(
                !qualifies_default_context(&surface),
                "mutation {index} must invalidate qualification"
            );
        }
    }
    #[test]
    fn dom_cannot_assert_default_context() {
        let mut value = json!({"account":"fixture@example.invalid", "workspace":null,
            "diagnostic":default_scope().diagnostic});
        let surface: ScopeSurface = serde_json::from_value(value.clone()).unwrap();
        assert!(!surface.default_workspace);
        value["default_workspace"] = json!(true);
        assert!(serde_json::from_value::<ScopeSurface>(value).is_err());
    }
    #[test]
    #[ignore = "requires an installed Chrome; creates a fresh diagnostic profile"]
    fn managed_browser_loads_http_and_completes_fetch() {
        use std::{io::Read, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(35);
            let mut completed = false;
            while Instant::now() < deadline {
                let (mut socket, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while request.len() < 4096 && !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    if socket.read_exact(&mut byte).is_err() {
                        break;
                    }
                    request.push(byte[0]);
                }
                // Chromium can preconnect without sending an HTTP request.
                if !request.ends_with(b"\r\n\r\n") {
                    continue;
                }
                let fetch = request.starts_with(b"GET /fetch ");
                let request_text = String::from_utf8_lossy(&request).to_ascii_lowercase();
                let language = request_text
                    .lines()
                    .find_map(|line| line.strip_prefix("accept-language:"))
                    .map(str::trim);
                assert!(
                    language.is_some_and(|value| matches!(
                        value.split([',', ';']).next(),
                        Some("en" | "en-us")
                    )),
                    "managed browser must negotiate English independently of the OS language: {language:?}"
                );
                let body = if fetch {
                    "network-fixture-ok"
                } else {
                    "<!doctype html><title>pending</title><script>fetch('/fetch').then(r=>r.text()).then(t=>document.title=t)</script>"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes());
                if fetch {
                    completed = true;
                    break;
                }
            }
            completed
        });
        let profile = std::env::temp_dir().join(format!(
            "cxweb-network-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        cxweb_platform::state::protected_directory(&profile).unwrap();
        let executable = cxweb_platform::state::installed_browser().unwrap();
        let mut browser = ManagedBrowser::launch(&executable, &profile, false).unwrap();
        let target = browser
            .call("Target.createTarget", json!({"url":origin}), None)
            .unwrap();
        let page = browser
            .attach(
                target["targetId"].as_str().unwrap().to_owned(),
                false,
                false,
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(25);
        let mut loaded = false;
        while Instant::now() < deadline {
            let value = browser.call("Runtime.evaluate", json!({"expression":"document.title === 'network-fixture-ok'", "returnByValue":true}), Some(&page.session)).unwrap();
            if value["result"]["value"] == true {
                loaded = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = browser.close();
        drop(browser);
        let served = server.join().unwrap();
        assert!(
            served,
            "managed Chrome did not complete the local HTTP fetch"
        );
        assert!(loaded, "managed Chrome did not render the fetch result");
    }
    #[test]
    fn framing_handles_split_reads_and_multiple_messages() {
        let mut reader = BufReader::with_capacity(2, &b"{\"a\":1}\0{\"b\":2}\0"[..]);
        assert_eq!(read_frame(&mut reader).unwrap(), b"{\"a\":1}");
        assert_eq!(read_frame(&mut reader).unwrap(), b"{\"b\":2}");
        assert!(read_frame(&mut reader).is_err());
    }
    #[test]
    fn refuses_unbounded_frames() {
        assert!(read_frame(&mut &vec![b'x'; MAX_FRAME + 1][..]).is_err());
    }
}
