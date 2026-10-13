//! The git plugin: where a pane's working directory sits in a git worktree,
//! and what the repository holding it is in the middle of.
//!
//! Every value comes out of files: the upward walk for `.git`, the `HEAD` it
//! names, and the marker files an interrupted operation leaves behind. Nothing
//! here runs git or reads the index, so a sweep costs one `readlink` per pane
//! and two `stat`s per repository, and the repositories are shared — a dozen
//! panes in one worktree are one entry, which is what keeps the sweep flat in
//! panes rather than in repositories.
//!
//! Values are published on refresh and the format registry reads the snapshot,
//! so expanding a status format never touches the filesystem.

use std::collections::{HashMap, HashSet};
use std::ffi::{CString, OsStr};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use std::io;

use super::{Host, PaneId, PaneValues, Plugin, Variable};

/// The variables this plugin answers for, in the spelling `README.md` fixes.
const VARIABLES: &[Variable] = &[
    Variable::new(c"git_worktree"),
    Variable::new(c"git_worktree_path"),
    Variable::new(c"git_subdir"),
    Variable::new(c"git_repo"),
    Variable::new(c"git_branch"),
    Variable::new(c"git_head"),
    Variable::new(c"git_action"),
    Variable::new(c"git_action_step"),
    Variable::new(c"git_action_total"),
];

/// How often every pane's working directory is read back. Slower than the
/// agent sweep: a `cd` is a keystroke away from being noticed either way, and
/// this one is doing filesystem work rather than reading `/proc`.
const INTERVAL: Duration = Duration::from_millis(500);

/// What the plugin publishes for one pane. Every field is empty for a pane
/// that is not in a repository, which is what lets a format branch on
/// `#{?git_worktree,…}` rather than on the plugin being enabled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PaneGit {
    worktree: CString,
    worktree_path: CString,
    subdir: CString,
    repo: CString,
    branch: CString,
    head: CString,
    action: CString,
    action_step: CString,
    action_total: CString,
}

/// Two caches keep the sweep cheap: the upward walk memoised by working
/// directory, and the repository state memoised by git directory. Published
/// per-pane values belong to the format registry.
pub struct GitPlugin {
    roots: HashMap<PathBuf, Discovery>,
    repos: HashMap<PathBuf, Repo>,
}

impl GitPlugin {
    pub fn new() -> Self {
        GitPlugin {
            roots: HashMap::new(),
            repos: HashMap::new(),
        }
    }

    /// The worktree a directory belongs to, walking up for it only the first
    /// time a directory is seen. A directory in no repository is not cached,
    /// so a repository created later is still found.
    fn discover(&mut self, cwd: &Path) -> Option<Discovery> {
        if let Some(found) = self.roots.get(cwd) {
            return Some(found.clone());
        }
        let found = Discovery::find(cwd)?;
        self.roots.insert(cwd.to_path_buf(), found.clone());
        Some(found)
    }

    /// The repository state behind a discovery, re-read only when the git
    /// directory has moved since the last read. A repository mid-operation is
    /// re-read every sweep instead: its progress counters live in a
    /// subdirectory, which the git directory's own timestamp says nothing
    /// about. `None` means the git directory is gone.
    fn repo(&mut self, found: &Discovery) -> Option<Repo> {
        let stamp = stamp(&found.gitdir)?;
        if let Some(repo) = self
            .repos
            .get(&found.gitdir)
            .filter(|repo| repo.stamp == stamp && repo.action.is_empty())
        {
            return Some(repo.clone());
        }
        let repo = Repo::read(found, stamp);
        self.repos.insert(found.gitdir.clone(), repo.clone());
        Some(repo)
    }

    /// Everything published for a pane sitting in `cwd`.
    fn pane_git(&mut self, cwd: &Path) -> PaneGit {
        let Some(found) = self.discover(cwd) else {
            return PaneGit::default();
        };
        let Some(repo) = self.repo(&found) else {
            self.roots.remove(cwd);
            self.repos.remove(&found.gitdir);
            return PaneGit::default();
        };
        PaneGit {
            worktree: file_name(&found.worktree),
            worktree_path: CString::new(found.worktree.as_os_str().as_bytes()).unwrap_or_default(),
            subdir: subdir(&found.worktree, cwd),
            repo: found.repo,
            head: match repo.branch.is_empty() {
                true => repo.commit.clone(),
                false => repo.branch.clone(),
            },
            branch: repo.branch,
            action: repo.action,
            action_step: repo.step,
            action_total: repo.total,
        }
    }
}

impl Default for GitPlugin {
    fn default() -> Self {
        GitPlugin::new()
    }
}

impl Plugin for GitPlugin {
    fn name(&self) -> &'static str {
        "git"
    }
    fn variables(&self) -> &'static [Variable] {
        VARIABLES
    }
    fn interval(&self) -> Duration {
        INTERVAL
    }

    fn refresh(&mut self, host: &dyn Host, output: &mut PaneValues) -> io::Result<()> {
        let mut live_cwds = HashSet::new();
        let mut live_gitdirs = HashSet::new();
        for id in host.pane_ids()? {
            let Some(cwd) = host.pane_cwd(id) else {
                continue;
            };
            let value = self.pane_git(&cwd);
            if let Some(found) = self.roots.get(&cwd) {
                live_gitdirs.insert(found.gitdir.clone());
            }
            live_cwds.insert(cwd);
            value.publish(id, output)?;
        }
        self.roots.retain(|cwd, _| live_cwds.contains(cwd));
        self.repos.retain(|gitdir, _| live_gitdirs.contains(gitdir));
        Ok(())
    }
}

impl PaneGit {
    fn publish(self, pane: PaneId, output: &mut PaneValues) -> io::Result<()> {
        for (key, value) in [
            (c"git_worktree", self.worktree),
            (c"git_worktree_path", self.worktree_path),
            (c"git_subdir", self.subdir),
            (c"git_repo", self.repo),
            (c"git_branch", self.branch),
            (c"git_head", self.head),
            (c"git_action", self.action),
            (c"git_action_step", self.action_step),
            (c"git_action_total", self.action_total),
        ] {
            output.set(pane, key, value)?;
        }
        Ok(())
    }
}

/// Where the upward walk stopped: the worktree root, the git directory it
/// names, and the repository that directory belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Discovery {
    worktree: PathBuf,
    gitdir: PathBuf,
    repo: CString,
    /// Whether refs live in a reftable rather than in files, which is what
    /// makes reading `HEAD` as a file meaningless.
    reftable: bool,
}

impl Discovery {
    /// The first worktree at or above `cwd`, or nothing if the walk reaches
    /// the filesystem root without finding one.
    fn find(cwd: &Path) -> Option<Discovery> {
        let mut dir = cwd;
        loop {
            if let Some(gitdir) = git_dir_at(dir) {
                let common = common_dir(&gitdir);
                return Some(Discovery {
                    worktree: dir.to_path_buf(),
                    repo: repo_name(&common),
                    reftable: common.join("reftable").is_dir(),
                    gitdir,
                });
            }
            dir = dir.parent()?;
        }
    }
}

/// The modification times a repository's cheap state hangs off: the git
/// directory itself, which moves when a marker file appears or goes, and
/// `HEAD`, which moves on checkout.
type Stamp = (Option<SystemTime>, Option<SystemTime>);

/// What the last read of a repository found. `commit` is only filled in for a
/// detached HEAD, since a branch name is the better answer whenever there is
/// one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Repo {
    stamp: Stamp,
    branch: CString,
    commit: CString,
    action: CString,
    step: CString,
    total: CString,
}

impl Repo {
    fn read(found: &Discovery, stamp: Stamp) -> Repo {
        let (action, step, total) = read_action(&found.gitdir);
        let (mut branch, commit) = match found.reftable {
            true => (CString::default(), CString::default()),
            false => read_head(&found.gitdir),
        };
        if branch.is_empty() {
            branch = rebased_branch(&found.gitdir);
        }
        Repo {
            stamp,
            branch,
            commit,
            action,
            step,
            total,
        }
    }
}

/// The timestamps a repository's state is re-read on, or nothing once the git
/// directory has gone — a worktree removed under the pane's feet.
fn stamp(gitdir: &Path) -> Option<Stamp> {
    let dir = fs::metadata(gitdir).ok()?;
    if !dir.is_dir() {
        return None;
    }
    let head = fs::metadata(gitdir.join("HEAD"))
        .ok()
        .and_then(|head| head.modified().ok());
    Some((dir.modified().ok(), head))
}

/// The git directory a worktree root holds: `.git` itself when it is a
/// directory, or the directory the `.git` file points at when the worktree is
/// a linked one.
fn git_dir_at(dir: &Path) -> Option<PathBuf> {
    let dot = dir.join(".git");
    let meta = fs::metadata(&dot).ok()?;
    if meta.is_dir() {
        return Some(dot);
    }
    if !meta.is_file() {
        return None;
    }
    let text = fs::read(&dot).ok()?;
    let named = text
        .split(|&byte| byte == b'\n')
        .find_map(|line| line.strip_prefix(b"gitdir:"))?;
    let named = trim_ascii(named);
    match named.is_empty() {
        true => None,
        false => Some(normalize(&dir.join(OsStr::from_bytes(named)))),
    }
}

/// The git directory the whole repository shares, which is the given one
/// unless it is a linked worktree's.
fn common_dir(gitdir: &Path) -> PathBuf {
    let text = fs::read(gitdir.join("commondir")).unwrap_or_default();
    let named = trim_ascii(text.split(|&byte| byte == b'\n').next().unwrap_or_default());
    match named.is_empty() {
        true => gitdir.to_path_buf(),
        false => normalize(&gitdir.join(OsStr::from_bytes(named))),
    }
}

/// The repository's name: the directory holding its common git directory, or
/// the bare directory's own name with the conventional suffix dropped.
fn repo_name(common: &Path) -> CString {
    let name = file_name(common);
    if name.as_c_str() == c".git" {
        return common.parent().map(file_name).unwrap_or_default();
    }
    CString::new(
        name.to_bytes()
            .strip_suffix(b".git")
            .unwrap_or(name.to_bytes()),
    )
    .expect("a path component has no interior NUL")
}

/// The branch `HEAD` names and, when it names none, the short form of the
/// commit it is parked on.
fn read_head(gitdir: &Path) -> (CString, CString) {
    let Ok(text) = fs::read(gitdir.join("HEAD")) else {
        return (CString::default(), CString::default());
    };
    let line = trim_ascii(text.split(|&byte| byte == b'\n').next().unwrap_or_default());
    if let Some(reference) = line.strip_prefix(b"ref:") {
        let reference = trim_ascii(reference);
        let branch = reference.strip_prefix(b"refs/heads/").unwrap_or(reference);
        // What a reftable repository leaves in the file for readers that
        // predate it; there is no branch name here to be had.
        if branch == b".invalid" {
            return (CString::default(), CString::default());
        }
        return (
            CString::new(branch).expect("a branch name has no NUL"),
            CString::default(),
        );
    }
    match line.len() >= 7 && line.iter().all(u8::is_ascii_hexdigit) {
        true => (
            CString::default(),
            CString::new(&line[..7]).expect("a commit id has no NUL"),
        ),
        false => (CString::default(), CString::default()),
    }
}

/// The branch an interrupted rebase is rebuilding. HEAD is detached for the
/// length of the operation, and this is the name the pane was on before it
/// started.
fn rebased_branch(gitdir: &Path) -> CString {
    for dir in ["rebase-merge", "rebase-apply"] {
        let Ok(text) = fs::read(gitdir.join(dir).join("head-name")) else {
            continue;
        };
        let name = trim_ascii(text.split(|&byte| byte == b'\n').next().unwrap_or_default());
        if let Some(branch) = name.strip_prefix(b"refs/heads/") {
            return CString::new(branch).expect("a branch name has no NUL");
        }
    }
    CString::default()
}

/// The operation the repository is in the middle of, with the step it has
/// reached and the number of steps it has, for the ones that count.
///
/// The two rebase backends are one `rebase` here. The marker that looks like
/// it separates an interactive rebase from a plain one — `rebase-merge/
/// interactive` — is written for every rebase the merge backend runs, so
/// reporting an interactive rebase from it would be wrong for the common
/// case rather than right for the rare one.
fn read_action(gitdir: &Path) -> (CString, CString, CString) {
    let merge = gitdir.join("rebase-merge");
    if merge.is_dir() {
        return (
            c"rebase".to_owned(),
            count(&merge.join("msgnum")),
            count(&merge.join("end")),
        );
    }
    let apply = gitdir.join("rebase-apply");
    if apply.is_dir() {
        let action = match apply.join("rebasing").exists() {
            true => "rebase",
            false => "am",
        };
        return (
            CString::new(action).expect("an action name has no NUL"),
            count(&apply.join("next")),
            count(&apply.join("last")),
        );
    }
    for (marker, action) in [
        ("MERGE_HEAD", "merge"),
        ("BISECT_LOG", "bisect"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
    ] {
        if gitdir.join(marker).exists() {
            return (
                CString::new(action).expect("an action name has no NUL"),
                CString::default(),
                CString::default(),
            );
        }
    }
    (CString::default(), CString::default(), CString::default())
}

/// A progress counter, or nothing when the file is missing or holds something
/// that is not one.
fn count(path: &Path) -> CString {
    let Ok(text) = fs::read(path) else {
        return CString::default();
    };
    let value = trim_ascii(text.split(|&byte| byte == b'\n').next().unwrap_or_default());
    match !value.is_empty() && value.iter().all(u8::is_ascii_digit) {
        true => CString::new(value).expect("a progress count has no NUL"),
        false => CString::default(),
    }
}

fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

/// The path from the worktree root down to the pane's directory, empty at the
/// root itself.
fn subdir(worktree: &Path, cwd: &Path) -> CString {
    cwd.strip_prefix(worktree)
        .ok()
        .and_then(|rest| CString::new(rest.as_os_str().as_bytes()).ok())
        .unwrap_or_default()
}

/// The last component of a path, as a string.
fn file_name(path: &Path) -> CString {
    path.file_name()
        .and_then(|name| CString::new(name.as_bytes()).ok())
        .unwrap_or_default()
}

/// A path with its `.` and `..` components resolved by name alone. The paths
/// this resolves are `commondir` and `.git`-file targets, which are written
/// relative to the file that names them and are never followed as a symlink
/// by git either.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !out.has_root() {
                    out.push(Component::ParentDir);
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    use std::os::unix::fs::PermissionsExt;

    use std::sync::atomic::{AtomicU64, Ordering};

    /// A directory tree that goes away with the test, for the file layouts
    /// these functions read.
    ///
    /// Directories are created one component at a time and given their mode
    /// explicitly so fixtures do not depend on the inherited process umask.
    struct Scratch {
        root: PathBuf,
    }

    impl Scratch {
        fn new() -> Scratch {
            let root = std::env::temp_dir().join(format!(
                "tmux-c2rs-git-{}-{:?}-{}",
                std::process::id(),
                std::thread::current().id(),
                {
                    static NEXT: AtomicU64 = AtomicU64::new(0);
                    NEXT.fetch_add(1, Ordering::Relaxed)
                }
            ));
            let _ = fs::remove_dir_all(&root);
            make_dir(&root);
            Scratch { root }
        }

        /// Write `text` to `path` under the scratch root, making the
        /// directories above it.
        fn write(&self, path: &str, text: &str) -> PathBuf {
            let full = self.root.join(path);
            make_dir(full.parent().expect("a parent"));
            fs::write(&full, text).expect("a file");
            full
        }

        fn dir(&self, path: &str) -> PathBuf {
            let full = self.root.join(path);
            make_dir(&full);
            full
        }
    }

    /// Make a directory and everything above it, each with a mode of its own.
    fn make_dir(path: &Path) {
        let mut at = PathBuf::new();
        for part in path.components() {
            at.push(part);
            if fs::create_dir(&at).is_ok() {
                let mode = fs::Permissions::from_mode(0o755);
                fs::set_permissions(&at, mode).expect("a directory mode");
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// The layout `git worktree add` leaves behind: a `.git` file in the
    /// linked worktree pointing into the main repository's git directory,
    /// which points back with `commondir`. The worktree is named by its own
    /// directory and the repository by the one holding the common git dir,
    /// which is the distinction the window label needs.
    #[test]
    fn a_linked_worktree_is_named_apart_from_its_repository() {
        let scratch = Scratch::new();
        let gitdir = scratch.dir("tiri/.git/worktrees/h1");
        fs::write(gitdir.join("commondir"), "../..\n").expect("commondir");
        fs::write(gitdir.join("HEAD"), "ref: refs/heads/h1\n").expect("HEAD");
        scratch.write(
            "h1/.git",
            &format!("gitdir: {}\n", gitdir.to_string_lossy()),
        );
        let deep = scratch.dir("h1/tmux-c2rs/src");

        let found = Discovery::find(&deep).expect("a worktree");
        assert_eq!(found.worktree, scratch.root.join("h1"));
        assert_eq!(found.gitdir, gitdir);
        assert_eq!(found.repo, c"tiri", "the repository, not the worktree");
        assert_eq!(file_name(&found.worktree), c"h1");
        assert_eq!(subdir(&found.worktree, &deep), c"tmux-c2rs/src");
        assert_eq!(
            subdir(&found.worktree, &scratch.root.join("h1")),
            c"",
            "nothing below the root"
        );

        let repo = Repo::read(&found, (None, None));
        assert_eq!(repo.branch, c"h1");
        assert_eq!(repo.action, c"");
    }

    /// A plain checkout: `.git` is the git directory, and the repository and
    /// the worktree are the same directory.
    #[test]
    fn a_plain_checkout_is_its_own_repository() {
        let scratch = Scratch::new();
        scratch.write("tiri/.git/HEAD", "ref: refs/heads/master\n");

        let found = Discovery::find(&scratch.dir("tiri")).expect("a worktree");
        assert_eq!(found.repo, c"tiri");
        assert_eq!(found.gitdir, scratch.root.join("tiri/.git"));
        assert_eq!(Repo::read(&found, (None, None)).branch, c"master");
    }

    /// A directory in no repository at all, which is every pane whose window
    /// label falls back to its working directory.
    #[test]
    fn a_directory_outside_a_repository_finds_nothing() {
        let scratch = Scratch::new();
        assert_eq!(Discovery::find(&scratch.dir("elsewhere")), None);
    }

    /// A detached HEAD reports the commit rather than a branch, and `git_head`
    /// is whichever of the two there is.
    #[test]
    fn a_detached_head_reports_its_commit() {
        let scratch = Scratch::new();
        scratch.write(
            "repo/.git/HEAD",
            "38b63b01c0ffee00c0ffee00c0ffee00c0ffee00\n",
        );

        let found = Discovery::find(&scratch.dir("repo")).expect("a worktree");
        let repo = Repo::read(&found, (None, None));
        assert_eq!(repo.branch, c"");
        assert_eq!(repo.commit, c"38b63b0");
    }

    /// A rebase stopped part way through, which is the state a multiplexer is
    /// best placed to show: the operation, how far it has got, and the branch
    /// it is rebuilding — HEAD is detached throughout, so the branch comes
    /// from the operation's own state.
    #[test]
    fn an_interrupted_rebase_reports_its_progress() {
        let scratch = Scratch::new();
        scratch.write(
            "repo/.git/HEAD",
            "38b63b01c0ffee00c0ffee00c0ffee00c0ffee00\n",
        );
        scratch.write("repo/.git/rebase-merge/head-name", "refs/heads/h1\n");
        scratch.write("repo/.git/rebase-merge/msgnum", "2\n");
        scratch.write("repo/.git/rebase-merge/end", "7\n");

        let found = Discovery::find(&scratch.dir("repo")).expect("a worktree");
        let repo = Repo::read(&found, (None, None));
        assert_eq!(repo.action, c"rebase");
        assert_eq!((repo.step.as_c_str(), repo.total.as_c_str()), (c"2", c"7"));
        assert_eq!(repo.branch, c"h1", "the branch being rebased");
    }

    /// The markers an interrupted merge, bisect, cherry-pick or revert leave,
    /// none of which count steps.
    #[test]
    fn the_other_operations_are_named_by_their_markers() {
        let scratch = Scratch::new();
        scratch.write("repo/.git/HEAD", "ref: refs/heads/master\n");
        let gitdir = scratch.root.join("repo/.git");

        for (marker, action) in [
            ("MERGE_HEAD", "merge"),
            ("BISECT_LOG", "bisect"),
            ("CHERRY_PICK_HEAD", "cherry-pick"),
            ("REVERT_HEAD", "revert"),
        ] {
            fs::write(gitdir.join(marker), "").expect("a marker");
            assert_eq!(
                read_action(&gitdir),
                (
                    CString::new(action).expect("an action has no NUL"),
                    CString::default(),
                    CString::default(),
                )
            );
            fs::remove_file(gitdir.join(marker)).expect("a marker");
        }
        assert_eq!(
            read_action(&gitdir),
            (CString::default(), CString::default(), CString::default()),
            "an idle repository"
        );
    }

    /// A repository whose refs are in a reftable has no branch name in a file
    /// to read, and says nothing rather than reporting the placeholder git
    /// leaves in `HEAD` for readers that predate it.
    #[test]
    fn a_reftable_repository_reports_no_branch() {
        let scratch = Scratch::new();
        scratch.write("repo/.git/HEAD", "ref: refs/heads/.invalid\n");
        scratch.dir("repo/.git/reftable");

        let found = Discovery::find(&scratch.dir("repo")).expect("a worktree");
        assert!(found.reftable);
        let repo = Repo::read(&found, (None, None));
        assert_eq!((repo.branch.as_c_str(), repo.commit.as_c_str()), (c"", c""));
    }

    #[test]
    fn publishes_all_declared_variables() {
        let mut output = PaneValues::new(VARIABLES);
        PaneGit::default().publish(PaneId(1), &mut output).unwrap();
        assert_eq!(output.panes[&1].len(), VARIABLES.len());
        assert!(output.panes[&1].values().all(|value| value.is_empty()));
    }

    /// Relative paths in `commondir` and in a linked worktree's `.git` are
    /// resolved by name, without asking the filesystem.
    #[test]
    fn relative_git_paths_are_resolved_by_name() {
        assert_eq!(
            normalize(Path::new("/a/b/.git/worktrees/h1/../..")),
            Path::new("/a/b/.git")
        );
        assert_eq!(normalize(Path::new("/a/./b/")), Path::new("/a/b"));
        assert_eq!(repo_name(Path::new("/a/tiri/.git")), c"tiri");
        assert_eq!(repo_name(Path::new("/a/tiri.git")), c"tiri", "a bare repo");
    }

    #[test]
    fn repository_names_retain_non_utf8_bytes() {
        let path = PathBuf::from(OsString::from_vec(b"/a/repo-\xff/.git".to_vec()));
        assert_eq!(repo_name(&path).to_bytes(), b"repo-\xff");
    }
}
