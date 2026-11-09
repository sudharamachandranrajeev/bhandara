//! Package-manager caches. Each one is cleaned by its own tool's command,
//! never by deleting its folder; commands that need root are only shown.

use serde::Serialize;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Cache {
    pub id: &'static str,
    pub label: &'static str,
    /// What happens when it is cleaned.
    pub note: &'static str,
    /// The command as a person would type it.
    pub command: String,
    /// Space used by the cache folder, when it can be read.
    pub bytes: Option<u64>,
    /// Bhandara can run it (the tool is installed and needs no root).
    pub runnable: bool,
    pub root: bool,
}

struct Entry {
    id: &'static str,
    label: &'static str,
    note: &'static str,
    program: &'static str,
    /// Run one after another.
    steps: &'static [&'static [&'static str]],
    root: bool,
    /// Only shown, never run (interactive or too broad).
    show_only: bool,
    dir: fn(&Dirs) -> Option<PathBuf>,
}

struct Dirs {
    home: Option<PathBuf>,
    cache: Option<PathBuf>,
    local: Option<PathBuf>,
}

impl Dirs {
    fn load() -> Self {
        let home = crate::clean::home_dir();
        let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(".cache")));
        Dirs { home, cache, local: std::env::var_os("LOCALAPPDATA").map(PathBuf::from) }
    }

    fn cache_or_local(&self, unix: &str, windows: &str) -> Option<PathBuf> {
        if cfg!(windows) { self.local.as_ref().map(|l| l.join(windows)) } else { self.cache.as_ref().map(|c| c.join(unix)) }
    }
}

const ENTRIES: &[Entry] = &[
    Entry {
        id: "mise",
        label: "mise tool versions and downloads",
        note: "Removes tool versions no config uses, then the download cache.",
        program: "mise",
        steps: &[&["prune", "-y"], &["cache", "clear"]],
        root: false,
        show_only: false,
        dir: |d| d.cache_or_local("mise", "mise\\cache"),
    },
    Entry {
        id: "npm",
        label: "npm cache",
        note: "Packages download again when a project installs them.",
        program: "npm",
        steps: &[&["cache", "clean", "--force"]],
        root: false,
        show_only: false,
        dir: |d| if cfg!(windows) { d.local.as_ref().map(|l| l.join("npm-cache")) } else { d.home.as_ref().map(|h| h.join(".npm/_cacache")) },
    },
    Entry {
        id: "uv",
        label: "uv cache",
        note: "Removes cache entries no longer used.",
        program: "uv",
        steps: &[&["cache", "prune"]],
        root: false,
        show_only: false,
        dir: |d| d.cache_or_local("uv", "uv\\cache"),
    },
    Entry {
        id: "pip",
        label: "pip cache",
        note: "Wheels download again when needed.",
        program: "pip",
        steps: &[&["cache", "purge"]],
        root: false,
        show_only: false,
        dir: |d| d.cache_or_local("pip", "pip\\cache"),
    },
    Entry {
        id: "yarn",
        label: "Yarn cache",
        note: "Packages download again when a project installs them.",
        program: "yarn",
        steps: &[&["cache", "clean"]],
        root: false,
        show_only: false,
        dir: |d| d.cache_or_local("yarn", "Yarn\\Cache"),
    },
    Entry {
        id: "pub",
        label: "Dart and Flutter package cache",
        note: "Every Dart package downloads again on the next pub get. Offline builds will fail until then.",
        program: "flutter",
        steps: &[&["pub", "cache", "clean", "-f"]],
        root: false,
        show_only: false,
        dir: |d| if cfg!(windows) { d.local.as_ref().map(|l| l.join("Pub\\Cache")) } else { d.home.as_ref().map(|h| h.join(".pub-cache")) },
    },
    Entry {
        id: "pacman",
        label: "pacman package cache",
        note: "Keeps the newest copy of each package, so a downgrade is still possible. Needs pacman-contrib.",
        program: "paccache",
        steps: &[&["-rk1"]],
        root: true,
        show_only: true,
        dir: |_| Some(PathBuf::from("/var/cache/pacman/pkg")),
    },
    Entry {
        id: "yay",
        label: "AUR build cache (yay)",
        note: "Asks before removing anything.",
        program: "yay",
        steps: &[&["-Sc"]],
        root: false,
        show_only: true,
        dir: |d| d.cache.as_ref().map(|c| c.join("yay")),
    },
    Entry {
        id: "journal",
        label: "System logs (journal)",
        note: "Keeps the newest 500 MB of logs.",
        program: "journalctl",
        steps: &[&["--vacuum-size=500M"]],
        root: true,
        show_only: true,
        dir: |_| Some(PathBuf::from("/var/log/journal")),
    },
    Entry {
        id: "docker",
        label: "Docker images and build cache",
        note: "Shown only: images, volumes and build cache can hold work. Check docker system df first.",
        program: "docker",
        steps: &[&["system", "df"]],
        root: false,
        show_only: true,
        dir: |_| None,
    },
];

/// Caches whose tool is installed or whose folder exists.
pub fn list() -> Vec<Cache> {
    let dirs = Dirs::load();
    ENTRIES
        .iter()
        .filter_map(|e| {
            let installed = find_program(e.program).is_some();
            let dir = (e.dir)(&dirs).filter(|d| d.is_dir());
            if !installed && dir.is_none() {
                return None;
            }
            let bytes = dir.as_deref().map(crate::clean::dir_bytes);
            let sudo = if e.root { "sudo " } else { "" };
            let command = e.steps.iter().map(|s| format!("{sudo}{} {}", e.program, s.join(" "))).collect::<Vec<_>>().join(" && ");
            Some(Cache { id: e.id, label: e.label, note: e.note, command, bytes, runnable: installed && !e.root && !e.show_only, root: e.root })
        })
        .collect()
}

/// Runs a cache's own clean command. Only ids from the catalogue run; root
/// and show-only entries are refused.
pub fn run(id: &str) -> io::Result<String> {
    let e = ENTRIES.iter().find(|e| e.id == id).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown cache"))?;
    if e.root || e.show_only {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "run this one yourself in a terminal"));
    }
    let program = find_program(e.program).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{} is not installed", e.program)))?;
    let mut log = String::new();
    for step in e.steps {
        let out = Command::new(&program).args(*step).output()?;
        log.push_str(&String::from_utf8_lossy(&out.stdout));
        log.push_str(&String::from_utf8_lossy(&out.stderr));
        if !out.status.success() {
            return Err(io::Error::other(format!("{} {} failed: {}", e.program, step.join(" "), log.trim())));
        }
    }
    Ok(log.trim().to_string())
}

/// Finds a program on PATH, plus mise's shims, which a desktop session's
/// PATH often lacks.
fn find_program(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = crate::clean::home_dir() {
        dirs.push(home.join(".local/share/mise/shims"));
        dirs.push(home.join(".local/bin"));
    }
    let names: Vec<String> =
        if cfg!(windows) { ["exe", "cmd", "bat"].iter().map(|ext| format!("{name}.{ext}")).collect() } else { vec![name.to_string()] };
    dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_and_show_only_never_run() {
        for id in ["pacman", "journal", "docker", "yay"] {
            assert_eq!(run(id).unwrap_err().kind(), io::ErrorKind::PermissionDenied, "{id}");
        }
        assert_eq!(run("rm -rf /").unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn listed_commands_read_like_a_terminal() {
        for c in list() {
            assert!(!c.command.is_empty());
            assert_eq!(c.root, c.command.starts_with("sudo "), "{}", c.command);
            assert!(!(c.runnable && c.root));
        }
    }
}
