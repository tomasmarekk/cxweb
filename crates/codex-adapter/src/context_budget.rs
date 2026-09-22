//! Explicit local operating limits. These never represent measured web usage.
pub const MAX_PROMPT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalContextBudget {
    normal_bytes: usize,
    summary_bytes: usize,
    estimated_tokens: u32,
}
impl LocalContextBudget {
    /// Reserve ample room for Codex's full-history compaction request. The
    /// ordinary response budget is deliberately lower so a single large tool
    /// result does not strand the task before compaction can run.
    /// The token field is a four-bytes-per-token sizing estimate for native
    /// compaction metadata; the byte limits remain authoritative at execution.
    pub const DIAGNOSTIC: Self = Self {
        normal_bytes: 256 * 1024,
        summary_bytes: MAX_PROMPT_BYTES,
        estimated_tokens: 64 * 1024,
    };

    pub fn new(normal_bytes: usize, summary_bytes: usize) -> Result<Self, &'static str> {
        if normal_bytes < 4096 || summary_bytes <= normal_bytes || summary_bytes > MAX_PROMPT_BYTES
        {
            return Err("E_CONTEXT_BUDGET_CONFIG");
        }
        Ok(Self {
            normal_bytes,
            summary_bytes,
            estimated_tokens: (normal_bytes / 4) as u32,
        })
    }
    pub fn normal_bytes(self) -> usize {
        self.normal_bytes
    }
    pub fn summary_bytes(self) -> usize {
        self.summary_bytes
    }
    pub fn estimated_tokens(self) -> u32 {
        self.estimated_tokens
    }
}
