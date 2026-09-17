//! Syntax-only recognizer for the pinned native apply_patch grammar. No file IO.
//! Contract: openai/codex rust-v0.153.4 core/assets/tools/apply_patch.lark.
use sha2::{Digest, Sha256};

pub fn recognizes(definition: &str) -> bool {
    format!("{:x}", Sha256::digest(definition.as_bytes()))
        == "d6367f4826ed608c424b0a308f3d6163527df63c22513d089b91863552f8bfeb"
}

pub fn valid(input: &str) -> bool {
    if input.contains(['\r', '\0']) {
        return false;
    }
    let input = input.strip_suffix('\n').unwrap_or(input);
    let lines: Vec<&str> = input.split('\n').collect();
    if lines.len() < 3 || lines[0] != "*** Begin Patch" || lines.last() != Some(&"*** End Patch") {
        return false;
    }
    let end = lines.len() - 1;
    let mut i = 1;
    let mut hunks = 0;
    while i < end {
        let line = lines[i];
        if filename(line, "*** Add File: ") {
            i += 1;
            let start = i;
            while i < end && lines[i].starts_with('+') {
                i += 1;
            }
            if i == start {
                return false;
            }
        } else if filename(line, "*** Delete File: ") {
            i += 1;
        } else if filename(line, "*** Update File: ") {
            i += 1;
            if i < end && filename(lines[i], "*** Move to: ") {
                i += 1;
            }
            let start = i;
            while i < end
                && (lines[i].starts_with(['+', '-', ' '])
                    || lines[i] == "@@"
                    || lines[i].strip_prefix("@@ ").is_some_and(|s| !s.is_empty()))
            {
                i += 1;
            }
            if i > start && i < end && lines[i] == "*** End of File" {
                i += 1;
            }
        } else {
            return false;
        }
        hunks += 1;
    }
    hunks > 0
}

fn filename(line: &str, prefix: &str) -> bool {
    line.strip_prefix(prefix).is_some_and(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_multifile_and_unicode_payload_remains_literal() {
        assert!(valid(
            "*** Begin Patch\n*** Add File: 🦀.txt\n+first\n+\n*** Update File: a\n*** Move to: b\n@@ context\n old\n-new\n+next\n*** End of File\n*** Delete File: c\n*** End Patch\n"
        ));
    }
    #[test]
    fn rejects_trailing_shell_empty_hunks_and_bad_framing() {
        for invalid in [
            "*** Begin Patch\n*** End Patch",
            "*** Begin Patch\n*** Add File: a\n*** End Patch",
            "*** Begin Patch\n*** Delete File: a\n*** End Patch\nsh command",
            "*** Begin Patch\n*** Update File: a\n*** End of File\n*** End Patch",
        ] {
            assert!(!valid(invalid));
        }
    }
}
