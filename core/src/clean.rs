//! Shodhana: finds build output that rebuilds itself and removes it safely.
//!
//! A folder is a candidate only when a marker file proves what made it (a
//! `pubspec.yaml` next to `build/`, a `CACHEDIR.TAG` inside `target/`); a
//! name alone never matches. Every candidate then passes a set of guards
//! (recent work, git-tracked files, a running build, an unignored Syncthing
//! folder, a mount point) before it may be removed, and is checked again at
//! removal time.

use crate::index::{MountKind, Mounts, allocated, inode, is_reparse_point, modified};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// The CACHEDIR.TAG standard: <https://bford.info/cachedir/>.
const CACHEDIR_SIGNATURE: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";
/// Release builds worth keeping a copy of before their folder goes.
const RELEASE_EXTENSIONS: &[&str] = &["apk", "aab", "appimage", "deb", "rpm", "msi", "exe", "dmg", "ipa"];
/// Upper bound on entries read when dating a project's own files.
const PROJECT_WALK_LIMIT: usize = 50_000;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    Cargo,
    Flutter,
    Node,
    Gradle,
    PythonVenv,
    PyCache,
    CacheTag,
}

impl Rule {
    pub const ALL: [Rule; 7] = [Rule::Cargo, Rule::Flutter, Rule::Node, Rule::Gradle, Rule::PythonVenv, Rule::PyCache, Rule::CacheTag];

    pub fn label(self) -> &'static str {
        match self {
            Rule::Cargo => "Rust build output",
            Rule::Flutter => "Flutter build output",
            Rule::Node => "Node packages",
            Rule::Gradle => "Gradle build output",
            Rule::PythonVenv => "Python virtual environment",
            Rule::PyCache => "Python bytecode cache",
            Rule::CacheTag => "Tagged cache folder",
        }
    }

    pub fn why_safe(self) -> &'static str {
        match self {
            Rule::Cargo => "Cargo marked this folder as a cache (CACHEDIR.TAG). It holds compiled output only.",
            Rule::Flutter => "Flutter writes build/ and .dart_tool/ next to pubspec.yaml; both are generated.",
            Rule::Node => "node_modules sits next to package.json and is installed from it.",
            Rule::Gradle => "Gradle writes build/ and .gradle/ next to its build script; both are generated.",
            Rule::PythonVenv => "pyvenv.cfg marks a virtual environment; the project lists what it needs.",
            Rule::PyCache => "Holds only .pyc files, which Python writes on import.",
            Rule::CacheTag => "The folder is marked as a cache (CACHEDIR.TAG) by the program that made it.",
        }
    }

    pub fn comes_back(self) -> &'static str {
        match self {
            Rule::Cargo => "The next cargo build (or tauri build) recreates it. The first build is slower.",
            Rule::Flutter => "The next flutter build or flutter run recreates it after flutter pub get.",
            Rule::Node => "npm install (or pnpm/yarn install) recreates it.",
            Rule::Gradle => "The next Gradle build recreates it.",
            Rule::PythonVenv => "Recreate it with python -m venv and install the project's requirements.",
            Rule::PyCache => "Python recreates it the next time the code runs.",
            Rule::CacheTag => "The program that made it fills it again when needed.",
        }
    }
}

/// Which rule, if any, proves that `path` (a folder named `name` inside
/// `parent`) is generated output.
pub fn detect(parent: &Path, name: &str, path: &Path) -> Option<Rule> {
    let has = |file: &str| parent.join(file).is_file();
    let gradle = || ["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"].iter().any(|f| has(f));
    match name {
        "target" if cachedir_tagged(path) => Some(Rule::Cargo),
        "build" | ".dart_tool" if has("pubspec.yaml") => Some(Rule::Flutter),
        "node_modules" if has("package.json") => Some(Rule::Node),
        "build" | ".gradle" if gradle() => Some(Rule::Gradle),
        ".venv" | "venv"
            if path.join("pyvenv.cfg").is_file() && ["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"].iter().any(|f| has(f)) =>
        {
            Some(Rule::PythonVenv)
        }
        "__pycache__" if only_bytecode(path) => Some(Rule::PyCache),
        // uv tags its virtual environments too; those go by the venv rule only.
        _ if cachedir_tagged(path) && !path.join("pyvenv.cfg").exists() => Some(Rule::CacheTag),
        _ => None,
    }
}

fn cachedir_tagged(dir: &Path) -> bool {
    let mut head = [0u8; 43];
    fs::File::open(dir.join("CACHEDIR.TAG")).and_then(|mut f| f.read_exact(&mut head)).is_ok() && head[..] == *CACHEDIR_SIGNATURE
}

fn only_bytecode(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else { return false };
    let mut any = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let is_file = entry.file_type().is_ok_and(|t| t.is_file());
        if !is_file || !name.to_string_lossy().ends_with(".pyc") {
            return false;
        }
        any = true;
    }
    any
}

/// Why a candidate is left alone. Guards cannot be overridden per item.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum Skip {
    /// The project changed within the configured number of days.
    Active { days: u32 },
    /// Git tracks files inside it, so it is not pure output.
    GitTracked,
    /// A process is working inside the project.
    Running,
    /// Inside a Syncthing folder that does not ignore it: removing it would
    /// remove it on the other devices too.
    Synced { folder: PathBuf },
    /// It is, or holds, a mount point.
    MountPoint,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: u32,
    pub path: PathBuf,
    /// The folder holding the marker file.
    pub project: PathBuf,
    pub rule: Rule,
    /// Space on disk, hard links counted once.
    pub bytes: u64,
    pub files: u64,
    /// Newest change to the project's own files (not its build output), Unix seconds.
    pub project_changed: i64,
    pub skip: Option<Skip>,
    /// Release packages inside, copied to the backup folder before removal.
    pub releases: Vec<PathBuf>,
}

/// One Syncthing folder and where its ignore file lives.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SyncFolder {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Projects changed within this many days are left alone.
    pub active_days: u32,
    pub syncthing: Vec<SyncFolder>,
    pub excluded: Vec<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options { active_days: 14, syncthing: syncthing_folders(), excluded: Vec::new() }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub root: PathBuf,
    pub candidates: Vec<Candidate>,
    pub partial: bool,
    pub scanned_at: u64,
}

impl Scan {
    /// Bytes that may be removed now (candidates with no guard against them).
    pub fn reclaimable(&self) -> u64 {
        self.candidates.iter().filter(|c| c.skip.is_none()).map(|c| c.bytes).sum()
    }
}

/// Finds candidates under `root`, staying on its filesystem. `progress`
/// gets the number of candidates sized so far and the number found.
pub fn scan(root: &Path, options: &Options, cancel: &AtomicBool, progress: impl Fn(usize, usize) + Sync) -> io::Result<Scan> {
    let mounts = Mounts::load(root);
    fs::read_dir(root)?;
    let mut found: Vec<(PathBuf, PathBuf, Rule)> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut partial = false;
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            partial = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if !kind.is_dir() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".git" || options.excluded.contains(&path) || matches!(mounts.classify(&path), MountKind::Foreign) {
                continue;
            }
            if let Ok(meta) = entry.metadata()
                && is_reparse_point(&meta)
            {
                continue;
            }
            match detect(&dir, &name, &path) {
                Some(rule) => found.push((path, dir.clone(), rule)),
                None => stack.push(path),
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));

    let links = Mutex::new(HashSet::new());
    let (next, done) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let total = found.len();
    progress(0, total);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get() * 2).min(total.max(1));
    let sized: Vec<(usize, Size)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let (found, links, next, done, progress, mounts) = (&found, &links, &next, &done, &progress, &mounts);
                scope.spawn(move || {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some((path, _, _)) = found.get(i) else { break };
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        out.push((i, size_of(path, mounts, links)));
                        progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).flatten().collect()
    });
    partial |= cancel.load(Ordering::Relaxed);
    let mut sizes: HashMap<usize, Size> = sized.into_iter().collect();

    let output_dirs: HashSet<PathBuf> = found.iter().map(|(p, _, _)| p.clone()).collect();
    let running = running_dirs();
    let mut project_changed: HashMap<PathBuf, i64> = HashMap::new();
    let now = crate::health::now() as i64;
    let mut candidates = Vec::new();
    for (i, (path, project, rule)) in found.into_iter().enumerate() {
        let Some(size) = sizes.remove(&i) else { continue };
        let changed = *project_changed.entry(project.clone()).or_insert_with(|| newest_change(&project, &output_dirs));
        let skip = guard(&path, &project, changed, now, size.mount, &running, options);
        candidates.push(Candidate {
            id: candidates.len() as u32,
            path,
            project,
            rule,
            bytes: size.bytes,
            files: size.files,
            project_changed: changed,
            skip,
            releases: size.releases,
        });
    }
    candidates.sort_by_key(|c| std::cmp::Reverse(c.bytes));
    for (i, c) in candidates.iter_mut().enumerate() {
        c.id = i as u32;
    }
    Ok(Scan { root: root.to_path_buf(), candidates, partial, scanned_at: crate::health::now() })
}

#[derive(Default)]
struct Size {
    bytes: u64,
    files: u64,
    mount: bool,
    releases: Vec<PathBuf>,
}

fn size_of(dir: &Path, mounts: &Mounts, links: &Mutex<HashSet<(u64, u64)>>) -> Size {
    let mut size = Size { mount: mounts.is_mount(dir), ..Size::default() };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                let path = entry.path();
                if mounts.is_mount(&path) || is_reparse_point(&meta) {
                    size.mount = true;
                    continue;
                }
                size.bytes += allocated(&meta);
                stack.push(path);
            } else if meta.is_file() {
                if let Some(key) = inode(&meta)
                    && !links.lock().unwrap().insert(key)
                {
                    continue;
                }
                size.bytes += allocated(&meta);
                size.files += 1;
                let path = entry.path();
                if is_release(&path) {
                    size.releases.push(path);
                }
            }
        }
    }
    size
}

/// Space on disk used under `dir`, staying on its filesystem.
pub(crate) fn dir_bytes(dir: &Path) -> u64 {
    size_of(dir, &Mounts::load(dir), &Mutex::new(HashSet::new())).bytes
}

fn is_release(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    RELEASE_EXTENSIONS.contains(&ext.as_str())
        && path.components().any(|c| c.as_os_str().to_string_lossy().to_lowercase().contains("release"))
}

/// Newest change among the project's own files, leaving out build output
/// and `.git`.
fn newest_change(project: &Path, output: &HashSet<PathBuf>) -> i64 {
    let mut newest = 0;
    let mut seen = 0;
    let mut stack = vec![project.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > PROJECT_WALK_LIMIT {
                return newest;
            }
            let path = entry.path();
            if output.contains(&path) || entry.file_name() == ".git" {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            // Files only: a folder's time also changes when build output
            // inside it is deleted, which is not work on the project.
            if meta.is_file() {
                newest = newest.max(modified(&meta));
            } else if meta.is_dir() && !is_reparse_point(&meta) {
                stack.push(path);
            }
        }
    }
    newest
}

fn guard(path: &Path, project: &Path, changed: i64, now: i64, mount: bool, running: &[PathBuf], options: &Options) -> Option<Skip> {
    if mount {
        return Some(Skip::MountPoint);
    }
    if running.iter().any(|cwd| cwd.starts_with(project)) {
        return Some(Skip::Running);
    }
    if git_tracked(path) {
        return Some(Skip::GitTracked);
    }
    if let Some(folder) = options.syncthing.iter().find(|f| path.starts_with(&f.path))
        && !syncthing_ignores(&folder.path, path)
    {
        return Some(Skip::Synced { folder: folder.path.clone() });
    }
    let age_days = ((now - changed).max(0) / 86_400) as u32;
    (age_days < options.active_days).then_some(Skip::Active { days: age_days })
}

/// Working folders of running processes (Linux and Android). Empty
/// elsewhere, where this guard does not apply.
fn running_dirs() -> Vec<PathBuf> {
    let Ok(procs) = fs::read_dir("/proc") else { return Vec::new() };
    procs
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|e| fs::read_link(e.path().join("cwd")).ok())
        .collect()
}

/// True when git tracks any file under `path`. False when there is no repo
/// or no git.
fn git_tracked(path: &Path) -> bool {
    let Some(repo) = path.ancestors().skip(1).find(|a| a.join(".git").exists()) else { return false };
    let Ok(rel) = path.strip_prefix(repo) else { return false };
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z", "--"])
        .arg(rel)
        .output()
        .is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
}

// ---------------------------------------------------------------- Syncthing

/// Syncthing folders configured for this user, read from its config.xml.
pub fn syncthing_folders() -> Vec<SyncFolder> {
    let home = home_dir();
    let mut configs = Vec::new();
    if let Some(home) = &home {
        configs.push(home.join(".local/state/syncthing/config.xml"));
        configs.push(home.join(".config/syncthing/config.xml"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        configs.push(PathBuf::from(local).join("Syncthing/config.xml"));
    }
    configs.iter().find_map(|p| fs::read_to_string(p).ok()).map(|xml| parse_syncthing(&xml, home.as_deref())).unwrap_or_default()
}

fn parse_syncthing(xml: &str, home: Option<&Path>) -> Vec<SyncFolder> {
    let mut folders = Vec::new();
    for tag in xml.split("<folder ").skip(1) {
        let tag = tag.split('>').next().unwrap_or("");
        let (Some(id), Some(path)) = (attribute(tag, "id"), attribute(tag, "path")) else { continue };
        if path.is_empty() {
            continue;
        }
        let path = match (path.strip_prefix("~/").or(path.strip_prefix("~\\")), home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(&path),
        };
        folders.push(SyncFolder { id, path });
    }
    folders
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!(" {name}=\"")).map(|i| i + name.len() + 3).or_else(|| tag.starts_with(&format!("{name}=\"")).then_some(name.len() + 2))?;
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].replace("&amp;", "&").replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&#39;", "'").replace("&apos;", "'"))
}

/// Whether the folder's `.stignore` keeps `path` out of sync. Unknown
/// constructs (`#include`) count as "not ignored", which keeps the guard on.
pub fn syncthing_ignores(folder: &Path, path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(folder.join(".stignore")) else { return false };
    let Ok(rel) = path.strip_prefix(folder) else { return false };
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    stignore_matches(&text, &parts)
}

fn stignore_matches(text: &str, parts: &[String]) -> bool {
    let mut patterns = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if line.starts_with("#include") {
            return false;
        }
        let (mut pattern, mut negate, mut fold) = (line, false, false);
        loop {
            if let Some(rest) = pattern.strip_prefix('!') {
                (pattern, negate) = (rest, true);
            } else if let Some(rest) = pattern.strip_prefix("(?d)") {
                pattern = rest;
            } else if let Some(rest) = pattern.strip_prefix("(?i)") {
                (pattern, fold) = (rest, true);
            } else {
                break;
            }
        }
        patterns.push((pattern.to_string(), negate, fold));
    }
    // Syncthing checks a folder before its contents: an ignored parent hides
    // everything inside; an explicitly included parent lets the check go on.
    for depth in 1..=parts.len() {
        let sub = &parts[..depth];
        let first = patterns.iter().find(|(p, _, fold)| pattern_matches(p, sub, *fold));
        if let Some((_, negate, _)) = first
            && !negate
        {
            return true;
        }
    }
    false
}

fn pattern_matches(pattern: &str, path: &[String], fold: bool) -> bool {
    let norm = |s: &str| if fold { s.to_lowercase() } else { s.to_string() };
    let joined = norm(&path.join("/"));
    let pattern = norm(pattern.trim_end_matches('/'));
    if let Some(anchored) = pattern.strip_prefix('/') {
        return glob(anchored, &joined);
    }
    // Unanchored: matches the path's tail, aligned on a folder boundary.
    (0..path.len()).any(|start| glob(&pattern, &norm(&path[start..].join("/"))))
}

/// `*` matches within one name, `**` across names, `?` one character.
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    fn go(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') if p.get(1) == Some(&'*') => (0..=t.len()).any(|i| go(&p[2..], &t[i..])),
            Some('*') => (0..=t.len()).take_while(|&i| i == 0 || t[i - 1] != '/').any(|i| go(&p[1..], &t[i..])),
            Some('?') => !t.is_empty() && t[0] != '/' && go(&p[1..], &t[1..]),
            Some(c) => t.first() == Some(c) && go(&p[1..], &t[1..]),
        }
    }
    go(&p, &t)
}

// ---------------------------------------------------------------- removal

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Move to the Trash or Recycle Bin: undoable, frees nothing until emptied.
    #[default]
    Trash,
    /// Remove for good: frees the space now.
    Permanent,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub at: u64,
    pub path: PathBuf,
    pub rule: Rule,
    pub bytes: u64,
    pub mode: Mode,
    /// Release packages copied out first.
    #[serde(default)]
    pub backed_up: Vec<PathBuf>,
    /// True when run by the schedule rather than by hand.
    #[serde(default)]
    pub scheduled: bool,
}

/// Removes one candidate after checking it again on disk. Release packages
/// inside are first copied under `backup` (relative to the scan root); if
/// that fails nothing is removed.
pub fn remove(scan: &Scan, id: u32, mode: Mode, backup: Option<&Path>) -> io::Result<Receipt> {
    let refuse = |why: &str| Err(io::Error::new(io::ErrorKind::PermissionDenied, why.to_string()));
    let c = scan.candidates.get(id as usize).filter(|c| c.id == id).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not in this scan"))?;
    if c.skip.is_some() {
        return refuse("a guard protects this folder");
    }
    let path = &c.path;
    if !path.starts_with(&scan.root) || path == &scan.root || home_dir().is_some_and(|h| h.starts_with(path)) {
        return refuse("outside the scanned folder");
    }
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() || is_reparse_point(&meta) {
        return refuse("no longer a plain folder");
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else { return refuse("no parent folder") };
    if detect(parent, &name.to_string_lossy(), path) != Some(c.rule) {
        return refuse("its marker file is gone; scan again");
    }
    if Mounts::load(&scan.root).is_mount(path) {
        return refuse("a mount point cannot be removed");
    }
    let mut backed_up = Vec::new();
    if let Some(backup) = backup {
        for file in &c.releases {
            let rel = file.strip_prefix(&scan.root).unwrap_or(file);
            let dest = backup.join(rel);
            fs::create_dir_all(dest.parent().unwrap_or(backup))?;
            fs::copy(file, &dest)?;
            backed_up.push(dest);
        }
    }
    match mode {
        Mode::Permanent => fs::remove_dir_all(path)?,
        Mode::Trash => trash_path(path)?,
    }
    Ok(Receipt { at: crate::health::now(), path: path.clone(), rule: c.rule, bytes: c.bytes, mode, backed_up, scheduled: false })
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn trash_path(path: &Path) -> io::Result<()> {
    trash::delete(path).map_err(io::Error::other)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn trash_path(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "this system has no Trash for these files"))
}

/// Appends receipts to a JSON-lines log.
pub fn append_receipts(log: &Path, receipts: &[Receipt]) -> io::Result<()> {
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = fs::OpenOptions::new().create(true).append(true).open(log)?;
    for r in receipts {
        writeln!(file, "{}", serde_json::to_string(r).map_err(io::Error::other)?)?;
    }
    Ok(())
}

/// The newest `limit` receipts, newest first.
pub fn read_receipts(log: &Path, limit: usize) -> Vec<Receipt> {
    let Ok(file) = fs::File::open(log) else { return Vec::new() };
    let mut all: Vec<Receipt> = io::BufReader::new(file).lines().map_while(Result::ok).filter_map(|l| serde_json::from_str(&l).ok()).collect();
    all.reverse();
    all.truncate(limit);
    all
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("bhandara-clean-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            TempTree(root)
        }

        fn file(&self, rel: &str, body: &[u8]) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, body).unwrap();
            path
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn options() -> Options {
        Options { active_days: 0, syncthing: Vec::new(), excluded: Vec::new() }
    }

    fn rules(scan: &Scan) -> Vec<(String, Rule)> {
        let mut out: Vec<_> = scan.candidates.iter().map(|c| (c.path.strip_prefix(&scan.root).unwrap().to_string_lossy().replace('\\', "/"), c.rule)).collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    fn tag() -> Vec<u8> {
        [CACHEDIR_SIGNATURE, b"\n# cargo\n"].concat()
    }

    #[test]
    fn markers_prove_each_rule() {
        let t = TempTree::new("rules");
        t.file("rs/Cargo.toml", b"");
        t.file("rs/target/CACHEDIR.TAG", &tag());
        t.file("rs/target/debug/app", &[0; 5000]);
        t.file("fl/pubspec.yaml", b"");
        t.file("fl/build/app.so", b"x");
        t.file("fl/.dart_tool/x", b"x");
        t.file("js/package.json", b"{}");
        t.file("js/node_modules/a/index.js", b"x");
        t.file("js/node_modules/a/node_modules/b/index.js", b"x");
        t.file("an/app/build.gradle.kts", b"");
        t.file("an/app/build/out.dex", b"x");
        t.file("py/requirements.txt", b"");
        t.file("py/.venv/pyvenv.cfg", b"");
        t.file("py/pkg/__pycache__/m.cpython-313.pyc", b"x");
        t.file("misc/thumbs/CACHEDIR.TAG", &tag());
        let scan = scan(&t.0, &options(), &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(
            rules(&scan),
            vec![
                ("an/app/build".into(), Rule::Gradle),
                ("fl/.dart_tool".into(), Rule::Flutter),
                ("fl/build".into(), Rule::Flutter),
                ("js/node_modules".into(), Rule::Node),
                ("misc/thumbs".into(), Rule::CacheTag),
                ("py/.venv".into(), Rule::PythonVenv),
                ("py/pkg/__pycache__".into(), Rule::PyCache),
                ("rs/target".into(), Rule::Cargo),
            ]
        );
        let target = scan.candidates.iter().find(|c| c.rule == Rule::Cargo).unwrap();
        assert!(target.bytes >= 5000 && target.files == 2);
    }

    #[test]
    fn names_without_markers_never_match() {
        let t = TempTree::new("names");
        // npkill#40: a home folder called node_modules, no package.json.
        t.file("node_modules/Documents/cv.pdf", b"x");
        t.file("site/build/index.html", b"x");
        t.file("rs/target/debug/app", b"x");
        t.file("rs/target/CACHEDIR.TAG", b"not the signature");
        t.file("py/.venv/lib/x.py", b"x");
        t.file("py/requirements.txt", b"");
        t.file("py/__pycache__/notes.txt", b"x");
        // A tagged venv with no project file next to it.
        t.file("lone/.venv/pyvenv.cfg", b"");
        t.file("lone/.venv/CACHEDIR.TAG", &tag());
        let scan = scan(&t.0, &options(), &AtomicBool::new(false), |_, _| {}).unwrap();
        assert!(scan.candidates.is_empty(), "{:?}", rules(&scan));
    }

    #[test]
    fn recent_projects_are_active() {
        let t = TempTree::new("active");
        t.file("fl/pubspec.yaml", b"");
        t.file("fl/build/a", b"x");
        let opts = Options { active_days: 14, ..options() };
        let scan = scan(&t.0, &opts, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(scan.candidates[0].skip, Some(Skip::Active { days: 0 }));
        assert_eq!(scan.reclaimable(), 0);
    }

    #[test]
    fn git_tracked_output_is_kept() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let t = TempTree::new("git");
        t.file("fl/pubspec.yaml", b"");
        t.file("fl/build/keep.txt", b"x");
        let git = |args: &[&str]| Command::new("git").arg("-C").arg(&t.0).args(args).output().unwrap();
        git(&["init", "-q"]);
        git(&["add", "fl/build/keep.txt"]);
        let scan = scan(&t.0, &options(), &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(scan.candidates[0].skip, Some(Skip::GitTracked));
    }

    #[test]
    fn unignored_syncthing_folders_are_guarded() {
        let t = TempTree::new("sync");
        t.file("a/pubspec.yaml", b"");
        t.file("a/build/x", b"x");
        let folder = SyncFolder { id: "work".into(), path: t.0.clone() };
        let opts = Options { syncthing: vec![folder], ..options() };
        let s = scan(&t.0, &opts, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(s.candidates[0].skip, Some(Skip::Synced { folder: t.0.clone() }));
        t.file(".stignore", b"// build output\n(?d)build\n");
        let s = scan(&t.0, &opts, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(s.candidates[0].skip, None);
    }

    #[test]
    fn stignore_rules() {
        let p = |s: &str| s.split('/').map(String::from).collect::<Vec<_>>();
        let text = "!/keep/dist\n(?d)dist\n/top/only\n*.tmp\n(?i)CACHE\n";
        assert!(stignore_matches(text, &p("app/dist")));
        assert!(!stignore_matches(text, &p("keep/dist")));
        assert!(stignore_matches(text, &p("top/only/x")));
        assert!(!stignore_matches(text, &p("sub/top/only")));
        assert!(stignore_matches(text, &p("a/b.tmp")));
        assert!(stignore_matches(text, &p("x/cache/y")));
        assert!(!stignore_matches(text, &p("app/build")));
        assert!(!stignore_matches("#include more\nbuild\n", &p("app/build")));
        assert!(stignore_matches("**/gen/**\n", &p("a/gen/b")));
    }

    #[test]
    fn syncthing_config_parse() {
        let xml = r#"<configuration><folder id="work" label="Work" path="~/Work" type="sendreceive">
            <device id="X"></device></folder><folder id="m" path="/srv/a &amp; b"></folder><folder id="e" path=""></folder></configuration>"#;
        let folders = parse_syncthing(xml, Some(Path::new("/home/u")));
        assert_eq!(
            folders,
            vec![SyncFolder { id: "work".into(), path: "/home/u/Work".into() }, SyncFolder { id: "m".into(), path: "/srv/a & b".into() }]
        );
    }

    #[test]
    fn remove_checks_again_and_backs_up_releases() {
        let t = TempTree::new("remove");
        t.file("fl/pubspec.yaml", b"");
        t.file("fl/build/app/outputs/flutter-apk/app-release.apk", b"apk");
        t.file("fl/build/app/intermediates/x.apk", b"not a release");
        t.file("fl/.dart_tool/x", b"x");
        let s = scan(&t.0, &options(), &AtomicBool::new(false), |_, _| {}).unwrap();
        let build = s.candidates.iter().find(|c| c.path.ends_with("build")).unwrap();
        assert_eq!(build.releases.len(), 1);
        let backup = t.0.join("backup");
        let receipt = remove(&s, build.id, Mode::Permanent, Some(&backup)).unwrap();
        assert!(!t.0.join("fl/build").exists());
        assert!(backup.join("fl/build/app/outputs/flutter-apk/app-release.apk").is_file());
        assert_eq!(receipt.backed_up.len(), 1);

        // The marker went away after the scan: refuse.
        let tool = s.candidates.iter().find(|c| c.path.ends_with(".dart_tool")).unwrap();
        fs::remove_file(t.0.join("fl/pubspec.yaml")).unwrap();
        assert!(remove(&s, tool.id, Mode::Permanent, None).is_err());
        assert!(t.0.join("fl/.dart_tool").exists());
    }

    #[test]
    fn guarded_candidates_are_refused() {
        let t = TempTree::new("refuse");
        t.file("fl/pubspec.yaml", b"");
        t.file("fl/build/x", b"x");
        let s = scan(&t.0, &Options { active_days: 14, ..options() }, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert!(remove(&s, 0, Mode::Permanent, None).is_err());
        assert!(t.0.join("fl/build").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let t = TempTree::new("links");
        let outside = TempTree::new("links-outside");
        outside.file("pubspec.yaml", b"");
        outside.file("build/x", b"x");
        std::os::unix::fs::symlink(&outside.0, t.0.join("link")).unwrap();
        let s = scan(&t.0, &options(), &AtomicBool::new(false), |_, _| {}).unwrap();
        assert!(s.candidates.is_empty());
    }

    #[test]
    fn receipts_round_trip() {
        let t = TempTree::new("receipts");
        let log = t.0.join("r/receipts.jsonl");
        let r = |n: u64| Receipt { at: n, path: format!("/p/{n}").into(), rule: Rule::Node, bytes: n, mode: Mode::Trash, backed_up: vec![], scheduled: false };
        append_receipts(&log, &[r(1), r(2)]).unwrap();
        append_receipts(&log, &[r(3)]).unwrap();
        assert_eq!(read_receipts(&log, 2), vec![r(3), r(2)]);
    }
}
