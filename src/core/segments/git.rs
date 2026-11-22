use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug)]
pub struct GitInfo {
    pub branch: String,
    pub status: GitStatus,
    pub ahead: u32,
    pub behind: u32,
    pub sha: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum GitStatus {
    Clean,
    Dirty,
    Conflicts,
}

pub struct GitSegment {
    show_sha: bool,
}

impl Default for GitSegment {
    fn default() -> Self {
        Self::new()
    }
}

impl GitSegment {
    pub fn new() -> Self {
        Self { show_sha: false }
    }

    pub fn with_sha(mut self, show_sha: bool) -> Self {
        self.show_sha = show_sha;
        self
    }

    fn get_git_info(&self, working_dir: &str) -> Option<GitInfo> {
        if working_dir.is_empty() {
            return None;
        }

        let mut cmd = Command::new("git");
        cmd.args(["--no-optional-locks", "status", "--porcelain=v2", "--branch"])
            .current_dir(working_dir);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let output = cmd.output().ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Some(Self::parse_porcelain_v2(&stdout, self.show_sha))
    }

    fn parse_porcelain_v2(output: &str, show_sha: bool) -> GitInfo {
        let mut branch: Option<String> = None;
        let mut oid: Option<&str> = None;
        let mut ahead = 0u32;
        let mut behind = 0u32;
        let mut has_dirty = false;
        let mut has_conflicts = false;

        for line in output.lines() {
            if let Some(rest) = line.strip_prefix("# branch.head ") {
                if rest == "(detached)" {
                    branch = Some("detached".to_string());
                } else if !rest.is_empty() {
                    branch = Some(rest.to_string());
                }
            } else if let Some(rest) = line.strip_prefix("# branch.oid ") {
                if rest != "(initial)" && !rest.is_empty() {
                    oid = Some(rest);
                }
            } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
                let mut parts = rest.split_whitespace();
                if let Some(a) = parts.next().and_then(|s| s.strip_prefix('+')) {
                    ahead = a.parse().unwrap_or(0);
                }
                if let Some(b) = parts.next().and_then(|s| s.strip_prefix('-')) {
                    behind = b.parse().unwrap_or(0);
                }
            } else if line.starts_with("u ") {
                has_conflicts = true;
            } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("? ") {
                has_dirty = true;
            }
        }

        let status = if has_conflicts {
            GitStatus::Conflicts
        } else if has_dirty {
            GitStatus::Dirty
        } else {
            GitStatus::Clean
        };

        let sha = if show_sha {
            oid.map(|full| full.chars().take(7).collect())
        } else {
            None
        };

        GitInfo {
            branch: branch.unwrap_or_else(|| "detached".to_string()),
            status,
            ahead,
            behind,
            sha,
        }
    }
}

impl Segment for GitSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let git_info = self.get_git_info(input.current_dir())?;

        let mut metadata = HashMap::new();
        metadata.insert("branch".to_string(), git_info.branch.clone());
        metadata.insert("status".to_string(), format!("{:?}", git_info.status));
        metadata.insert("ahead".to_string(), git_info.ahead.to_string());
        metadata.insert("behind".to_string(), git_info.behind.to_string());

        if let Some(ref sha) = git_info.sha {
            metadata.insert("sha".to_string(), sha.clone());
        }

        let primary = git_info.branch;
        let mut status_parts = Vec::new();

        match git_info.status {
            GitStatus::Clean => status_parts.push("✓".to_string()),
            GitStatus::Dirty => status_parts.push("●".to_string()),
            GitStatus::Conflicts => status_parts.push("⚠".to_string()),
        }

        if git_info.ahead > 0 {
            status_parts.push(format!("↑{}", git_info.ahead));
        }
        if git_info.behind > 0 {
            status_parts.push(format!("↓{}", git_info.behind));
        }

        if let Some(ref sha) = git_info.sha {
            status_parts.push(sha.clone());
        }

        if let Some(wt) = input.worktree_name() {
            metadata.insert("worktree".to_string(), wt.to_string());
            if primary.contains(wt) {
                status_parts.push("[wt]".to_string());
            } else {
                status_parts.push(format!("[wt:{}]", wt));
            }
        }

        Some(SegmentData {
            primary,
            secondary: status_parts.join(" "),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Git
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_repo_with_ahead_behind_and_sha() {
        let out = "\
# branch.oid 1234567890abcdef1234567890abcdef12345678
# branch.head main
# branch.upstream origin/main
# branch.ab +3 -12
";
        let info = GitSegment::parse_porcelain_v2(out, true);
        assert_eq!(info.branch, "main");
        assert_eq!(info.status, GitStatus::Clean);
        assert_eq!(info.ahead, 3);
        assert_eq!(info.behind, 12);
        assert_eq!(info.sha.as_deref(), Some("1234567"));
    }

    #[test]
    fn filename_containing_uu_aa_dd_is_dirty_not_conflict() {
        let out = "\
# branch.oid 1234567890abcdef1234567890abcdef12345678
# branch.head feature
1 .M N... 100644 100644 100644 abcdef0 abcdef0 src/UUID.rs
? README_AA_DD.md
";
        let info = GitSegment::parse_porcelain_v2(out, false);
        assert_eq!(info.branch, "feature");
        assert_eq!(info.status, GitStatus::Dirty);
        assert_eq!(info.sha, None);
    }

    #[test]
    fn unmerged_entry_reports_conflicts() {
        let out = "\
# branch.oid 1234567890abcdef1234567890abcdef12345678
# branch.head main
u DU N... 100644 000000 100644 100644 abcdef0 0000000 abcdef0 conflicted.rs
";
        let info = GitSegment::parse_porcelain_v2(out, false);
        assert_eq!(info.status, GitStatus::Conflicts);
    }

    #[test]
    fn handles_detached_head_and_initial_repo() {
        let detached = "\
# branch.oid deadbeef12345678
# branch.head (detached)
";
        let info = GitSegment::parse_porcelain_v2(detached, true);
        assert_eq!(info.branch, "detached");
        assert_eq!(info.sha.as_deref(), Some("deadbee"));

        let initial = "\
# branch.oid (initial)
# branch.head main
";
        let info = GitSegment::parse_porcelain_v2(initial, true);
        assert_eq!(info.branch, "main");
        assert_eq!(info.sha, None);
    }
}
