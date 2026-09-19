//! Current-user Task Scheduler supervision. No shell, saved password, elevated
//! principal or arbitrary PID termination. Installation must journal ownership
//! before activation; registered tasks are never removed merely by dropping UI.
use std::{io, path::Path};
use windows::{
    Win32::{
        Foundation::{FILETIME, RPC_E_CHANGED_MODE, SYSTEMTIME, VARIANT_FALSE, VARIANT_TRUE},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            SystemInformation::GetSystemTimeAsFileTime,
            TaskScheduler::*,
            Time::FileTimeToSystemTime,
            Variant::VARIANT,
        },
    },
    core::{BSTR, Interface},
};

struct Apartment(bool);
impl Apartment {
    fn enter() -> io::Result<Self> {
        // SAFETY: null reserved argument. Initialization is balanced on this
        // thread; an existing apartment keeps its original threading model.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self(false));
        }
        result.ok().map_err(com_error)?;
        Ok(Self(true))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: exactly one successful CoInitializeEx on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}
fn com_error(error: windows::core::Error) -> io::Error {
    io::Error::other(format!("E_TASK_SCHEDULER_{:08X}", error.code().0 as u32))
}
fn service() -> windows::core::Result<ITaskService> {
    // SAFETY: caller initialized COM; fixed local scheduler class. Empty variants
    // select the local machine/current user, never a caller-supplied destination.
    unsafe {
        let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
        let empty = VARIANT::default();
        service.Connect(&empty, &empty, &empty, &empty)?;
        Ok(service)
    }
}
fn local_path(path: &Path, file: bool) -> io::Result<String> {
    if !path.is_absolute() {
        return Err(io::Error::other("E_TASK_PATH"));
    }
    let canonical = path.canonicalize()?;
    if file != canonical.is_file() {
        return Err(io::Error::other("E_TASK_PATH"));
    }
    let text = canonical
        .to_str()
        .ok_or_else(|| io::Error::other("E_TASK_PATH"))?;
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    if text.as_bytes().get(1) != Some(&b':') || text.chars().any(char::is_control) {
        return Err(io::Error::other("E_TASK_PATH"));
    }
    Ok(text.to_owned())
}
fn quote(argument: &str) -> String {
    // Windows CommandLineToArgvW/CRT quoting, including trailing backslashes.
    let mut output = String::from("\"");
    let mut slashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            '\\',
            if character == '"' {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        output.push(character);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n('\\', slashes * 2));
    output.push('"');
    output
}

pub struct TaskPlan {
    name: String,
    xml: String,
    sid: String,
}
pub struct RegisteredRuntime {
    name: String,
    xml: String,
    planned_xml: String,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationReceipt {
    name: String,
    xml: String,
    planned_xml: String,
}

pub fn task_name(installation: &str) -> io::Result<String> {
    if installation.len() != 32 || !installation.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::other("E_TASK_INSTALLATION"));
    }
    Ok(format!(
        "cxweb-{}-{}",
        crate::state::current_sid()?,
        installation.to_ascii_lowercase()
    ))
}
impl RegistrationReceipt {
    pub fn matches_plan(&self, installation: &str, planned_xml: &str) -> io::Result<bool> {
        Ok(self.name == task_name(installation)?
            && self.planned_xml == planned_xml
            && !self.xml.is_empty()
            && self.xml.len() <= 128 * 1024
            && !self.xml.contains('\0'))
    }
    /// A receipt never recreates a task from stored XML. Reopening verifies the
    /// independently selected installation and the exact current OS definition.
    pub fn reopen(&self, installation: &str) -> io::Result<RegisteredRuntime> {
        if !self.matches_plan(installation, &self.planned_xml)?
            || self.planned_xml.len() > 128 * 1024
        {
            return Err(io::Error::other("E_TASK_RECEIPT"));
        }
        let task = RegisteredRuntime {
            name: self.name.clone(),
            xml: self.xml.clone(),
            planned_xml: self.planned_xml.clone(),
        };
        task.with_task(|_, _| Ok(()))?;
        Ok(task)
    }
}
pub struct TaskStatus {
    pub running: bool,
    pub last_result: i32,
    pub last_run: f64,
}

impl TaskPlan {
    /// Call only with the qualified installed executable and independently
    /// selected journal/config. The config may be absent after owned-only undo.
    pub fn new(
        installation: &str,
        executable: &Path,
        journal: &Path,
        config: &Path,
    ) -> io::Result<Self> {
        if !config.is_absolute() {
            return Err(io::Error::other("E_TASK_PATH"));
        }
        if installation.len() != 32 || !installation.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(io::Error::other("E_TASK_INSTALLATION"));
        }
        let executable = local_path(executable, true)?;
        let journal = local_path(journal, false)?;
        let config = crate::atomic_file::Snapshot::capture(config)?
            .path()
            .to_owned();
        let config = config
            .to_str()
            .ok_or_else(|| io::Error::other("E_TASK_PATH"))?;
        let config = config.strip_prefix(r"\\?\").unwrap_or(config);
        if config.as_bytes().get(1) != Some(&b':') || config.chars().any(char::is_control) {
            return Err(io::Error::other("E_TASK_PATH"));
        }
        let sid = crate::state::current_sid()?;
        let name = task_name(installation)?;
        let _apartment = Apartment::enter()?;
        // SAFETY: all interfaces originate from this thread's local scheduler.
        // BSTR/VARIANT parameters own their storage for each synchronous call.
        let xml = unsafe {
            let definition = service()
                .map_err(com_error)?
                .NewTask(0)
                .map_err(com_error)?;
            let configure = || -> windows::core::Result<()> {
                definition
                    .RegistrationInfo()?
                    .SetDescription(&BSTR::from(format!("cxweb runtime {installation}")))?;
                let principal = definition.Principal()?;
                principal.SetUserId(&BSTR::from(&sid))?;
                principal.SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)?;
                principal.SetRunLevel(TASK_RUNLEVEL_LUA)?;
                let trigger: ILogonTrigger =
                    definition.Triggers()?.Create(TASK_TRIGGER_LOGON)?.cast()?;
                trigger.SetUserId(&BSTR::from(&sid))?;
                trigger.SetEnabled(VARIANT_TRUE)?;
                definition
                    .Triggers()?
                    .Create(TASK_TRIGGER_REGISTRATION)?
                    .SetEnabled(VARIANT_TRUE)?;
                // RestartOnFailure did not restart an exited action in live
                // qualification. A periodic OS trigger plus IgnoreNew bounds
                // attempts while leaving an existing process untouched.
                let now = GetSystemTimeAsFileTime();
                let ticks = ((u64::from(now.dwHighDateTime) << 32) | u64::from(now.dwLowDateTime))
                    + 600_000_000;
                let future = FILETIME {
                    dwLowDateTime: ticks as u32,
                    dwHighDateTime: (ticks >> 32) as u32,
                };
                let mut time = SYSTEMTIME::default();
                FileTimeToSystemTime(&future, &mut time)?;
                let periodic = definition.Triggers()?.Create(TASK_TRIGGER_TIME)?;
                periodic.SetStartBoundary(&BSTR::from(format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                    time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
                )))?;
                periodic.Repetition()?.SetInterval(&BSTR::from("PT1M"))?;
                let settings = definition.Settings()?;
                settings.SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW)?;
                settings.SetExecutionTimeLimit(&BSTR::from("PT0S"))?;
                settings.SetDisallowStartIfOnBatteries(VARIANT_FALSE)?;
                settings.SetStopIfGoingOnBatteries(VARIANT_FALSE)?;
                settings.SetAllowHardTerminate(VARIANT_FALSE)?;
                settings.SetAllowDemandStart(VARIANT_TRUE)?;
                settings.SetStartWhenAvailable(VARIANT_TRUE)?;
                settings.SetEnabled(VARIANT_TRUE)?;
                let action: IExecAction = definition.Actions()?.Create(TASK_ACTION_EXEC)?.cast()?;
                action.SetPath(&BSTR::from(&executable))?;
                action.SetArguments(&BSTR::from(format!(
                    "--journal {} --config {}",
                    quote(&journal),
                    quote(config)
                )))?;
                Ok(())
            };
            configure().map_err(com_error)?;
            let mut xml = BSTR::new();
            definition.XmlText(&mut xml).map_err(com_error)?;
            xml.to_string()
        };
        Ok(Self { name, xml, sid })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn xml(&self) -> &str {
        &self.xml
    }
    /// Create-only registration starts the qualified runtime through its OS
    /// registration trigger; never replace an existing task of any owner.
    pub fn register(&self) -> io::Result<RegisteredRuntime> {
        let _apartment = Apartment::enter()?;
        // SAFETY: fixed local root folder, owned BSTRs, interactive-token logon
        // with no password and owner/SYSTEM access. The registration trigger starts it.
        unsafe {
            let folder = service()
                .map_err(com_error)?
                .GetFolder(&BSTR::from("\\"))
                .map_err(com_error)?;
            let descriptor = format!("O:{}D:P(A;;FA;;;SY)(A;;FA;;;{})", self.sid, self.sid);
            let task = folder
                .RegisterTask(
                    &BSTR::from(&self.name),
                    &BSTR::from(&self.xml),
                    TASK_CREATE.0,
                    &VARIANT::from(self.sid.as_str()),
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &VARIANT::from(descriptor.as_str()),
                )
                .map_err(com_error)?;
            Ok(RegisteredRuntime {
                name: self.name.clone(),
                xml: task.Xml().map_err(com_error)?.to_string(),
                planned_xml: self.xml.clone(),
            })
        }
    }
}
impl RegisteredRuntime {
    pub fn receipt(&self) -> RegistrationReceipt {
        RegistrationReceipt {
            name: self.name.clone(),
            xml: self.xml.clone(),
            planned_xml: self.planned_xml.clone(),
        }
    }
    fn with_task<T>(
        &self,
        action: impl FnOnce(&ITaskFolder, &IRegisteredTask) -> io::Result<T>,
    ) -> io::Result<T> {
        let _apartment = Apartment::enter()?;
        // SAFETY: local COM interfaces and owned names; the exact registered
        // definition is checked before any subsequent mutation or observation.
        unsafe {
            let folder = service()
                .map_err(com_error)?
                .GetFolder(&BSTR::from("\\"))
                .map_err(com_error)?;
            let task = folder.GetTask(&BSTR::from(&self.name)).map_err(com_error)?;
            if task.Xml().map_err(com_error)? != self.xml {
                return Err(io::Error::other("E_TASK_CHANGED"));
            }
            action(&folder, &task)
        }
    }
    pub fn start(&self) -> io::Result<()> {
        self.with_task(|_, task| {
            // SAFETY: ownership checked; default variant supplies no arguments.
            unsafe {
                task.Run(&VARIANT::default()).map_err(com_error)?;
            }
            Ok(())
        })
    }
    pub fn status(&self) -> io::Result<TaskStatus> {
        self.with_task(|_, task| {
            // SAFETY: read-only queries on the validated local task interface.
            unsafe {
                Ok(TaskStatus {
                    running: matches!(
                        task.State().map_err(com_error)?,
                        TASK_STATE_RUNNING | TASK_STATE_QUEUED
                    ),
                    last_result: task.LastTaskResult().map_err(com_error)?,
                    last_run: task.LastRunTime().map_err(com_error)?,
                })
            }
        })
    }
    /// Caller must also establish that no clients retain the route. This method
    /// never terminates the action and refuses running or queued instances.
    pub fn remove_stopped(&self) -> io::Result<()> {
        self.with_task(|folder, task| {
            // SAFETY: exact definition checked above. DeleteTask removes only
            // this registration; no arbitrary process stop is requested.
            unsafe {
                if matches!(
                    task.State().map_err(com_error)?,
                    TASK_STATE_RUNNING | TASK_STATE_QUEUED
                ) || task
                    .GetInstances(0)
                    .map_err(com_error)?
                    .Count()
                    .map_err(com_error)?
                    != 0
                {
                    return Err(io::Error::other("E_TASK_RUNNING"));
                }
                folder
                    .DeleteTask(&BSTR::from(&self.name), 0)
                    .map_err(com_error)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduler_arguments_preserve_trailing_slashes_and_quotes() {
        assert_eq!(
            quote("C:\\path with spaces\\"),
            "\"C:\\path with spaces\\\\\""
        );
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
    }
    #[test]
    fn persisted_receipt_is_bound_to_user_installation_and_original_plan() {
        let id = "a".repeat(32);
        let receipt = RegistrationReceipt {
            name: task_name(&id).unwrap(),
            xml: "registered-definition".into(),
            planned_xml: "original-plan".into(),
        };
        assert!(receipt.matches_plan(&id, "original-plan").unwrap());
        assert!(!receipt.matches_plan(&id, "changed-plan").unwrap());
        assert!(
            !receipt
                .matches_plan(&"b".repeat(32), "original-plan")
                .unwrap()
        );
        assert!(receipt.reopen(&"b".repeat(32)).is_err());
        assert!(task_name("../foreign").is_err());
    }
    #[test]
    fn task_plan_is_interactive_least_privilege_and_bounded() {
        let executable = std::env::current_exe().unwrap();
        let directory = std::env::temp_dir().join(format!(
            "cxweb-task-plan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        crate::state::protected_directory(&directory).unwrap();
        let plan = TaskPlan::new(
            &"a".repeat(32),
            &executable,
            &directory,
            &directory.join("absent-config.toml"),
        )
        .unwrap();
        for required in [
            "<LogonType>InteractiveToken</LogonType>",
            "<RunLevel>LeastPrivilege</RunLevel>",
            "<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>",
            "<Interval>PT1M</Interval>",
            "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
            "<AllowHardTerminate>false</AllowHardTerminate>",
        ] {
            assert!(plan.xml().contains(required), "missing setting {required}");
        }
        assert!(!plan.xml().contains("<Password>"));
        assert!(
            TaskPlan::new(
                "../foreign",
                &executable,
                &directory,
                &directory.join("config.toml")
            )
            .is_err()
        );
        std::fs::remove_dir(directory).unwrap();
    }
}
