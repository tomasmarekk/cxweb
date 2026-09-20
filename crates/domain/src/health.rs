//! Sanitized control-plane health. Unknown evidence never implies readiness.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Overall {
    Disconnected,
    Preflight,
    Authenticating,
    Discovering,
    RestartRequired,
    Ready,
    Busy,
    AuthRequired,
    RateLimited,
    Offline,
    Incompatible,
    ConfigConflict,
    Unavailable,
    Disconnecting,
    RemovalPendingRestart,
    DisconnectedComplete,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    Healthy,
    Degraded,
    Unavailable,
    #[default]
    Unknown,
    NotInstalled,
    RestartRequired,
    AuthRequired,
    Incompatible,
    Conflict,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    #[default]
    None,
    LocalProbe,
    PassiveBrowser,
    ClientHandshake,
    RequestSuccess,
    ManualLive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Connect,
    Cancel,
    OpenLogin,
    Check,
    Disconnect,
    Update,
    Details,
    None,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub state: ComponentState,
    pub observed_at: Option<String>,
    pub evidence: Evidence,
    pub code: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Components {
    pub runtime: Component,
    pub browser: Component,
    pub web_auth: Component,
    pub web_models: Component,
    pub native_upstream: Component,
    pub codex_app: Component,
    pub codex_cli: Component,
    pub config: Component,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Health {
    pub schema_version: String,
    pub revision: u64,
    pub overall: Overall,
    pub active_web_turns: u64,
    pub components: Components,
    pub suggested_action: Action,
}
impl Default for Health {
    fn default() -> Self {
        Self {
            schema_version: "webbridge.health.v1".into(),
            revision: 0,
            overall: Overall::Unavailable,
            active_web_turns: 0,
            components: Components::default(),
            suggested_action: Action::Check,
        }
    }
}
