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
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginObservation {
    pub official_page: bool,
    pub composer: bool,
    pub account_surface: bool,
    pub login_action: bool,
    pub selected_label: Option<String>,
}

pub struct ManagedBrowser {
    process: BrowserProcess,
    replies: Receiver<io::Result<Value>>,
    next_id: u64,
}

impl ManagedBrowser {
    pub fn launch(executable: &Path, profile: &Path, visible: bool) -> io::Result<Self> {
        let process = browser_process::launch(executable, profile, visible)?;
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
        })
    }

    pub fn pid(&self) -> u32 {
        self.process.pid
    }

    pub fn version(&mut self) -> io::Result<Value> {
        self.call("Browser.getVersion", json!({}), None)
    }

    pub fn open_login(&mut self) -> io::Result<ManagedPage> {
        let value = self.call(
            "Target.createTarget",
            json!({"url":"https://chatgpt.com/", "newWindow":true}),
            None,
        )?;
        let target = value["targetId"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing target identity"))?;
        self.attach(target.to_owned(), false)
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
    fn attach(&mut self, target: String, fixture: bool) -> io::Result<ManagedPage> {
        let result = self.call(
            "Target.attachToTarget",
            json!({"targetId":target,"flatten":true}),
            None,
        )?;
        let session = result["sessionId"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing page session"))?
            .to_owned();
        Ok(ManagedPage {
            target,
            session,
            fixture,
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
        let guarded = format!(
            "function(expectedOrigin, args) {{ if (location.origin !== expectedOrigin || (expectedOrigin === 'null' && location.href !== 'about:blank')) throw new Error('E_OFFICIAL_ORIGIN_REQUIRED'); return ({function})(...args); }}"
        );
        let result = self.call("Runtime.callFunctionOn", json!({"objectId":object,"functionDeclaration":guarded,"arguments":[{"value":if page.fixture {"null"} else {"https://chatgpt.com"}},{"value":arguments}],"returnByValue":true}), Some(&page.session));
        let _ = self.call(
            "Runtime.releaseObject",
            json!({"objectId":object}),
            Some(&page.session),
        );
        let result = result?;
        if result.get("exceptionDetails").is_some() {
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

    pub fn insert_prompt(&mut self, page: &ManagedPage, prompt: &str) -> io::Result<()> {
        if prompt.len() > 512 * 1024 {
            return Err(io::Error::other("E_CONTEXT_BUDGET"));
        }
        if self.dom(page, include_str!("dom/focus.js"), vec![])? != true {
            return Err(io::Error::other("E_COMPOSER_FOCUS"));
        }
        // Prompt is a CDP string argument, never executable JavaScript.
        self.call(
            "Input.insertText",
            json!({"text":prompt}),
            Some(&page.session),
        )?;
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
        self.dom(
            page,
            include_str!("dom/send.js"),
            vec![json!(prompt), json!(selected_model)],
        )?;
        Ok(())
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
        self.call("Target.closeTarget", json!({"targetId":page.target}), None)?;
        Ok(())
    }

    /// Development qualification only. Uses bundled synthetic markup in a fresh
    /// blank target. It cannot qualify selectors against a real account.
    pub fn probe_dom(&mut self) -> io::Result<Value> {
        let result = self.call("Target.createTarget", json!({"url":"about:blank"}), None)?;
        let target = result["targetId"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing fixture target"))?
            .to_owned();
        let page = self.attach(target, true)?;
        let result = self.run_dom_fixture(&page);
        let closed = self.close_page(page);
        let result = result?;
        closed?;
        Ok(result)
    }

    fn run_dom_fixture(&mut self, page: &ManagedPage) -> io::Result<Value> {
        let frame = self.call("Page.getFrameTree", json!({}), Some(&page.session))?;
        self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture.html")}), Some(&page.session))?;
        let baseline = self.baseline(page)?;
        let mut tracker =
            TurnTracker::new(baseline.clone(), "Fixture text mode").map_err(io::Error::other)?;
        let prompt = "Literal input: quotes \" ' ` ${never_execute()} <script>throw 1</script>\nUnicode: 🦀 🦀";
        self.insert_prompt(page, prompt)?;
        tracker.begin_submission().map_err(io::Error::other)?;
        self.press_send(page, prompt, &baseline.selected_model)?;
        let observation = self.observe(page, &baseline, prompt)?;
        match tracker.observe(observation).map_err(io::Error::other)? {
            Progress::Completed(text) if text == "fixture response" => {
                self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":include_str!("dom/fixture_stop.html")}), Some(&page.session))?;
                if !self.stop(page)? || self.baseline(page)?.generating || self.stop(page)? {
                    return Err(io::Error::other("E_CANCEL_FIXTURE"));
                }
                self.call("Page.setDocumentContent", json!({"frameId":frame["frameTree"]["frame"]["id"],"html":"<!doctype html><title>Selector drift fixture</title>"}), Some(&page.session))?;
                if self.baseline(page).is_ok() {
                    return Err(io::Error::other("E_SELECTOR_DRIFT_FIXTURE"));
                }
                Ok(
                    json!({"result":"PASS","evidence":"synthetic DOM only","literal_prompt":true,"historical_message_excluded":true,"completion_attributed":true,"stop_control":true,"selector_drift_rejected":true}),
                )
            }
            _ => Err(io::Error::other("E_DOM_FIXTURE")),
        }
    }

    pub fn close(&mut self) -> io::Result<()> {
        self.call("Browser.close", json!({}), None).map(|_| ())
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
                let fetch = request.starts_with(b"GET /fetch ");
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
            .attach(target["targetId"].as_str().unwrap().to_owned(), false)
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
