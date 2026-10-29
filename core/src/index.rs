//! Suchi: an in-memory index of a folder tree for the Files view.
//!
//! Every folder is kept, with whole-subtree totals. Files of `BIG_FILE` or
//! more are kept one by one; smaller files are only counted into their
//! folder, by type. On a typical home folder that keeps over 90% of the bytes
//! individually while using a fraction of the memory of a full file list.

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, Metadata};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Files at least this big are listed one by one.
pub const BIG_FILE: u64 = 1 << 20;
/// Unreadable folders beyond this many are counted, not listed.
const MAX_UNREADABLE: usize = 200;

pub type Id = u32;

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    /// Apparent size: what the files say they hold.
    pub size: u64,
    /// Space actually used on disk. Cloud-only placeholders count as 0.
    pub alloc: u64,
    /// Files, at any depth.
    pub count: u64,
}

impl Totals {
    fn add(&mut self, other: Totals) {
        self.size += other.size;
        self.alloc += other.alloc;
        self.count += other.count;
    }

    fn sub(&mut self, other: Totals) {
        self.size = self.size.saturating_sub(other.size);
        self.alloc = self.alloc.saturating_sub(other.alloc);
        self.count = self.count.saturating_sub(other.count);
    }
}

#[derive(Debug)]
struct Dir {
    name: String,
    parent: Option<Id>,
    children: Vec<Id>,
    /// Big files directly inside.
    files: Vec<Id>,
    /// Small files directly inside, by type.
    small: Vec<(u32, Totals)>,
    total: Totals,
    /// Newest change in the subtree, Unix seconds.
    modified: i64,
    /// A mount point of the same filesystem (a btrfs subvolume); never deleted.
    mount: bool,
    removed: bool,
}

#[derive(Debug)]
struct File {
    name: String,
    parent: Id,
    size: u64,
    alloc: u64,
    modified: i64,
    ext: u32,
    inode: Option<(u64, u64)>,
    removed: bool,
}

#[derive(Default, Debug)]
struct Tree {
    dirs: Vec<Dir>,
    files: Vec<File>,
    exts: Vec<String>,
    ext_ids: HashMap<String, u32>,
    unreadable: Vec<PathBuf>,
    unreadable_count: usize,
    partial: bool,
}

/// Shared by every worker during one scan.
struct Walk<'a> {
    mounts: &'a Mounts,
    excluded: &'a [PathBuf],
    cancel: &'a AtomicBool,
    links: &'a Mutex<HashSet<(u64, u64)>>,
}

impl Tree {
    fn push_dir(&mut self, name: String, parent: Option<Id>, mount: bool) -> Id {
        self.dirs.push(Dir {
            name,
            parent,
            children: Vec::new(),
            files: Vec::new(),
            small: Vec::new(),
            total: Totals::default(),
            modified: 0,
            mount,
            removed: false,
        });
        let id = (self.dirs.len() - 1) as Id;
        if let Some(parent) = parent {
            self.dirs[parent as usize].children.push(id);
        }
        id
    }

    fn ext_id(&mut self, name: &str) -> u32 {
        let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if let Some(&id) = self.ext_ids.get(&ext) {
            return id;
        }
        self.exts.push(ext.clone());
        let id = (self.exts.len() - 1) as u32;
        self.ext_ids.insert(ext, id);
        id
    }

    fn add_file(&mut self, dir: Id, name: String, meta: &Metadata, walk: &Walk) {
        let inode = inode(meta);
        // A hard-linked file is counted once, wherever it is seen first.
        if let Some(key) = inode
            && !walk.links.lock().unwrap().insert(key)
        {
            return;
        }
        let (size, alloc, modified) = (meta.len(), allocated(meta), modified(meta));
        let ext = self.ext_id(&name);
        let totals = Totals { size, alloc, count: 1 };
        if size >= BIG_FILE {
            self.files.push(File { name, parent: dir, size, alloc, modified, ext, inode, removed: false });
            let id = (self.files.len() - 1) as Id;
            self.dirs[dir as usize].files.push(id);
        } else {
            let small = &mut self.dirs[dir as usize].small;
            match small.iter_mut().find(|(e, _)| *e == ext) {
                Some((_, t)) => t.add(totals),
                None => small.push((ext, totals)),
            }
        }
        let d = &mut self.dirs[dir as usize];
        d.total.add(totals);
        d.modified = d.modified.max(modified);
    }

    fn note_unreadable(&mut self, path: PathBuf) {
        self.unreadable_count += 1;
        if self.unreadable.len() < MAX_UNREADABLE {
            self.unreadable.push(path);
        }
    }

    /// Scans `path` into a new folder under `parent`; returns its id. Totals
    /// are rolled up to the new folder but not beyond it.
    fn walk(&mut self, path: &Path, name: String, parent: Option<Id>, mount: bool, walk: &Walk) -> Id {
        let top = self.push_dir(name, parent, mount);
        let mut stack = vec![(path.to_path_buf(), top)];
        while let Some((dir_path, id)) = stack.pop() {
            if walk.cancel.load(Ordering::Relaxed) {
                self.partial = true;
                break;
            }
            let entries = match fs::read_dir(&dir_path) {
                Ok(entries) => entries,
                Err(_) => {
                    self.note_unreadable(dir_path);
                    continue;
                }
            };
            for entry in entries.flatten() {
                // Not followed: a symlink is neither a file nor a folder here.
                let Ok(meta) = entry.metadata() else { continue };
                let name = entry.file_name().to_string_lossy().into_owned();
                if meta.is_dir() {
                    let path = entry.path();
                    if let Some(mount) = walk.enter(&path, &meta) {
                        let child = self.push_dir(name, Some(id), mount);
                        stack.push((path, child));
                    }
                } else if meta.is_file() {
                    self.add_file(id, name, &meta, walk);
                }
            }
        }
        self.roll_up(top);
        top
    }

    /// Adds every folder created after `top` into its parent. Children are
    /// always created after their parents, so one reverse pass is enough.
    fn roll_up(&mut self, top: Id) {
        for id in (top as usize + 1..self.dirs.len()).rev() {
            let (parent, total, modified) = {
                let d = &self.dirs[id];
                (d.parent.expect("only the top folder has no parent") as usize, d.total, d.modified)
            };
            self.dirs[parent].total.add(total);
            self.dirs[parent].modified = self.dirs[parent].modified.max(modified);
        }
    }

    /// Moves a separately scanned tree (its folder 0 as the top) under
    /// `parent`, and adds its totals all the way up.
    fn graft(&mut self, other: Tree, parent: Id) -> Id {
        let (dir_offset, file_offset) = (self.dirs.len() as Id, self.files.len() as Id);
        let ext_map: Vec<u32> = other.exts.iter().map(|e| self.ext_id_exact(e)).collect();
        for mut d in other.dirs {
            d.parent = Some(d.parent.map_or(parent, |p| p + dir_offset));
            d.children.iter_mut().for_each(|c| *c += dir_offset);
            d.files.iter_mut().for_each(|f| *f += file_offset);
            d.small.iter_mut().for_each(|(e, _)| *e = ext_map[*e as usize]);
            self.dirs.push(d);
        }
        for mut f in other.files {
            f.parent += dir_offset;
            f.ext = ext_map[f.ext as usize];
            self.files.push(f);
        }
        self.dirs[parent as usize].children.push(dir_offset);
        let (total, modified) = (self.dirs[dir_offset as usize].total, self.dirs[dir_offset as usize].modified);
        self.add_up(parent, total, modified);
        self.unreadable_count += other.unreadable_count;
        for path in other.unreadable {
            if self.unreadable.len() < MAX_UNREADABLE {
                self.unreadable.push(path);
            }
        }
        self.partial |= other.partial;
        dir_offset
    }

    fn ext_id_exact(&mut self, ext: &str) -> u32 {
        if let Some(&id) = self.ext_ids.get(ext) {
            return id;
        }
        self.exts.push(ext.to_string());
        let id = (self.exts.len() - 1) as u32;
        self.ext_ids.insert(ext.to_string(), id);
        id
    }

    fn add_up(&mut self, mut id: Id, total: Totals, modified: i64) {
        loop {
            let d = &mut self.dirs[id as usize];
            d.total.add(total);
            d.modified = d.modified.max(modified);
            match d.parent {
                Some(p) => id = p,
                None => break,
            }
        }
    }

    fn sub_up(&mut self, mut id: Id, total: Totals) {
        loop {
            let d = &mut self.dirs[id as usize];
            d.total.sub(total);
            match d.parent {
                Some(p) => id = p,
                None => break,
            }
        }
    }
}

impl Walk<'_> {
    /// Whether to descend into a folder, and if so whether it is a
    /// same-filesystem mount point.
    fn enter(&self, path: &Path, meta: &Metadata) -> Option<bool> {
        if is_reparse_point(meta) || self.excluded.iter().any(|e| e == path) {
            return None;
        }
        match self.mounts.classify(path) {
            MountKind::Foreign => None,
            MountKind::SameFilesystem => Some(true),
            MountKind::None => Some(false),
        }
    }
}

/// What the index refers to: a folder or a listed (big) file.
#[derive(serde::Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum Item {
    Dir(Id),
    File(Id),
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub item: Item,
    pub name: String,
    #[serde(flatten)]
    pub total: Totals,
    pub modified: i64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Crumb {
    pub id: Id,
    pub name: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub id: Id,
    pub name: String,
    pub path: PathBuf,
    /// From the scan root down to this folder's parent.
    pub crumbs: Vec<Crumb>,
    #[serde(flatten)]
    pub total: Totals,
    pub modified: i64,
    pub mount: bool,
    /// Subfolders and listed files, biggest first.
    pub entries: Vec<Entry>,
    /// Files under `BIG_FILE` directly in this folder.
    pub small: Totals,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TypeStat {
    /// Lower-case extension without the dot; empty for none.
    pub ext: String,
    #[serde(flatten)]
    pub total: Totals,
    /// How many of these files are listed one by one.
    pub listed: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    pub id: Id,
    pub name: String,
    pub path: PathBuf,
    pub ext: String,
    pub size: u64,
    pub alloc: u64,
    pub modified: i64,
}

#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FileFilter {
    pub ext: Option<String>,
    pub min_size: u64,
    /// Only files last changed before this Unix time.
    pub modified_before: Option<i64>,
    pub limit: usize,
    /// Filter and sort by space used on disk instead of apparent size.
    pub on_disk: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub root: PathBuf,
    #[serde(flatten)]
    pub total: Totals,
    pub folders: usize,
    pub listed_files: usize,
    /// The scan was stopped before it finished.
    pub partial: bool,
    pub unreadable: Vec<PathBuf>,
    pub unreadable_count: usize,
    pub scanned_at: u64,
}

#[derive(Debug)]
pub struct Index {
    root: PathBuf,
    tree: Tree,
    scanned_at: u64,
}

/// Scans `root`, staying on its filesystem, skipping `excluded` folders.
/// Top-level folders are scanned in parallel; `progress(done, total)` counts
/// them. Setting `cancel` stops early with partial results.
pub fn scan(root: &Path, excluded: &[PathBuf], cancel: &AtomicBool, progress: impl Fn(usize, usize) + Sync) -> io::Result<Index> {
    let mounts = Mounts::load(root);
    let links = Mutex::new(HashSet::new());
    let walk = Walk { mounts: &mounts, excluded, cancel, links: &links };

    let mut tree = Tree::default();
    let top = tree.push_dir(display_name(root), None, false);
    let mut subdirs = Vec::new();
    for entry in fs::read_dir(root)?.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if meta.is_dir() {
            let path = entry.path();
            if let Some(mount) = walk.enter(&path, &meta) {
                subdirs.push((path, name, mount));
            }
        } else if meta.is_file() {
            tree.add_file(top, name, &meta, &walk);
        }
    }
    tree.roll_up(top);

    // Disk-bound work: a few more workers than cores keeps the queue full.
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get() * 2).min(subdirs.len().max(1));
    let (next, done) = (AtomicUsize::new(0), AtomicUsize::new(0));
    progress(0, subdirs.len());
    let parts: Vec<Tree> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let (subdirs, walk, next, done, progress) = (&subdirs, &walk, &next, &done, &progress);
                scope.spawn(move || {
                    let mut parts = Vec::new();
                    while let Some((path, name, mount)) = subdirs.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let mut part = Tree::default();
                        part.walk(path, name.clone(), None, *mount, walk);
                        parts.push(part);
                        progress(done.fetch_add(1, Ordering::Relaxed) + 1, subdirs.len());
                    }
                    parts
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).flatten().collect()
    });
    for part in parts {
        tree.graft(part, top);
    }
    tree.partial |= cancel.load(Ordering::Relaxed);
    Ok(Index { root: root.to_path_buf(), tree, scanned_at: crate::health::now() })
}

impl Index {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn summary(&self) -> Summary {
        let t = &self.tree;
        Summary {
            root: self.root.clone(),
            total: t.dirs[0].total,
            folders: t.dirs.iter().filter(|d| !d.removed).count(),
            listed_files: t.files.iter().filter(|f| !f.removed).count(),
            partial: t.partial,
            unreadable: t.unreadable.clone(),
            unreadable_count: t.unreadable_count,
            scanned_at: self.scanned_at,
        }
    }

    /// True while the item is still in the tree (not deleted or replaced by
    /// a rescan).
    pub fn is_live(&self, item: Item) -> bool {
        let mut dir = match item {
            Item::Dir(id) => id,
            Item::File(id) => match self.tree.files.get(id as usize) {
                Some(f) if !f.removed => f.parent,
                _ => return false,
            },
        };
        loop {
            match self.tree.dirs.get(dir as usize) {
                Some(d) if !d.removed => match d.parent {
                    Some(p) => dir = p,
                    None => return true,
                },
                _ => return false,
            }
        }
    }

    pub fn path(&self, item: Item) -> Option<PathBuf> {
        if !self.is_live(item) {
            return None;
        }
        let (mut names, mut dir) = match item {
            Item::Dir(id) => (Vec::new(), id),
            Item::File(id) => {
                let f = &self.tree.files[id as usize];
                (vec![f.name.as_str()], f.parent)
            }
        };
        while let Some(parent) = self.tree.dirs[dir as usize].parent {
            names.push(&self.tree.dirs[dir as usize].name);
            dir = parent;
        }
        Some(names.iter().rev().fold(self.root.clone(), |path, name| path.join(name)))
    }

    pub fn folder(&self, id: Id) -> Option<FolderView> {
        if !self.is_live(Item::Dir(id)) {
            return None;
        }
        let t = &self.tree;
        let d = &t.dirs[id as usize];
        let mut crumbs = Vec::new();
        let mut cursor = d.parent;
        while let Some(p) = cursor {
            crumbs.push(Crumb { id: p, name: t.dirs[p as usize].name.clone() });
            cursor = t.dirs[p as usize].parent;
        }
        crumbs.reverse();

        let mut entries: Vec<Entry> = d
            .children
            .iter()
            .map(|&c| {
                let child = &t.dirs[c as usize];
                Entry { item: Item::Dir(c), name: child.name.clone(), total: child.total, modified: child.modified }
            })
            .chain(d.files.iter().map(|&f| {
                let file = &t.files[f as usize];
                Entry {
                    item: Item::File(f),
                    name: file.name.clone(),
                    total: Totals { size: file.size, alloc: file.alloc, count: 1 },
                    modified: file.modified,
                }
            }))
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.total.alloc.max(e.total.size)));
        let mut small = Totals::default();
        d.small.iter().for_each(|(_, s)| small.add(*s));

        Some(FolderView {
            id,
            name: d.name.clone(),
            path: self.path(Item::Dir(id))?,
            crumbs,
            total: d.total,
            modified: d.modified,
            mount: d.mount,
            entries,
            small,
        })
    }

    /// Live folder ids, depth first from the root.
    fn live_dirs(&self) -> Vec<Id> {
        let mut out = Vec::new();
        let mut stack = vec![0];
        while let Some(id) = stack.pop() {
            out.push(id);
            stack.extend(&self.tree.dirs[id as usize].children);
        }
        out
    }

    fn live_files(&self) -> impl Iterator<Item = Id> + '_ {
        self.live_dirs().into_iter().flat_map(|d| self.tree.dirs[d as usize].files.clone())
    }

    /// Space by file type, biggest first.
    pub fn types(&self) -> Vec<TypeStat> {
        let mut by_ext: HashMap<u32, (Totals, u64)> = HashMap::new();
        for d in self.live_dirs() {
            for (ext, totals) in &self.tree.dirs[d as usize].small {
                by_ext.entry(*ext).or_default().0.add(*totals);
            }
        }
        for f in self.live_files() {
            let file = &self.tree.files[f as usize];
            let entry = by_ext.entry(file.ext).or_default();
            entry.0.add(Totals { size: file.size, alloc: file.alloc, count: 1 });
            entry.1 += 1;
        }
        let mut stats: Vec<TypeStat> = by_ext
            .into_iter()
            .map(|(ext, (total, listed))| TypeStat { ext: self.tree.exts[ext as usize].clone(), total, listed })
            .collect();
        stats.sort_by_key(|s| std::cmp::Reverse(s.total.alloc.max(s.total.size)));
        stats
    }

    /// Listed files matching `filter`, biggest first.
    pub fn files(&self, filter: &FileFilter) -> Vec<FileView> {
        let ext = filter.ext.as_ref().map(|e| e.to_lowercase());
        let measure = |f: Id| {
            let file = &self.tree.files[f as usize];
            if filter.on_disk { file.alloc } else { file.size }
        };
        let mut ids: Vec<Id> = self
            .live_files()
            .filter(|&f| {
                let file = &self.tree.files[f as usize];
                measure(f) >= filter.min_size
                    && filter.modified_before.is_none_or(|before| file.modified < before)
                    && ext.as_ref().is_none_or(|e| &self.tree.exts[file.ext as usize] == e)
            })
            .collect();
        ids.sort_by_key(|&f| std::cmp::Reverse(measure(f)));
        if filter.limit > 0 {
            ids.truncate(filter.limit);
        }
        ids.into_iter().filter_map(|f| self.file(f)).collect()
    }

    pub fn file(&self, id: Id) -> Option<FileView> {
        let path = self.path(Item::File(id))?;
        let f = &self.tree.files[id as usize];
        Some(FileView {
            id,
            name: f.name.clone(),
            path,
            ext: self.tree.exts[f.ext as usize].clone(),
            size: f.size,
            alloc: f.alloc,
            modified: f.modified,
        })
    }

    /// Listed files that share a size with another, one per inode, as
    /// candidates for `find_duplicates`.
    pub fn duplicate_candidates(&self) -> Vec<Vec<(Id, PathBuf)>> {
        let mut by_size: HashMap<u64, Vec<Id>> = HashMap::new();
        let mut inodes = HashSet::new();
        for f in self.live_files() {
            let file = &self.tree.files[f as usize];
            if file.inode.is_some_and(|i| !inodes.insert(i)) {
                continue;
            }
            by_size.entry(file.size).or_default().push(f);
        }
        by_size
            .into_values()
            .filter(|ids| ids.len() > 1)
            .map(|ids| ids.into_iter().filter_map(|f| Some((f, self.path(Item::File(f))?))).collect())
            .collect()
    }

    /// Scans one folder again and puts the result in place of the old one.
    pub fn rescan(&mut self, id: Id, excluded: &[PathBuf], cancel: &AtomicBool) -> io::Result<Id> {
        let path = self.path(Item::Dir(id)).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "folder no longer in the index"))?;
        let Some(parent) = self.tree.dirs[id as usize].parent else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "rescan the root with a new scan"));
        };
        let mounts = Mounts::load(&self.root);
        let links = Mutex::new(HashSet::new());
        let walk = Walk { mounts: &mounts, excluded, cancel, links: &links };
        let old = &self.tree.dirs[id as usize];
        let mut part = Tree::default();
        part.walk(&path, old.name.clone(), None, old.mount, &walk);
        self.detach(Item::Dir(id));
        Ok(self.tree.graft(part, parent))
    }

    /// Deletes an item from disk (Move to Trash; permanently on Android,
    /// which has no trash for ordinary files) and from the index. Returns
    /// the totals removed.
    pub fn delete(&mut self, item: Item) -> io::Result<Totals> {
        self.delete_with(item, delete_path)
    }

    fn delete_with(&mut self, item: Item, delete: impl FnOnce(&Path, bool) -> io::Result<()>) -> io::Result<Totals> {
        let refuse = |why: &str| Err(io::Error::new(io::ErrorKind::PermissionDenied, why.to_string()));
        let path = self.path(item).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no longer in the index"))?;
        if let Item::Dir(id) = item {
            if id == 0 {
                return refuse("the scan root cannot be deleted");
            }
            if self.tree.dirs[id as usize].mount {
                return refuse("a mount point cannot be deleted");
            }
        }
        if !path.starts_with(&self.root) || path == self.root {
            return refuse("outside the scanned folder");
        }
        delete(&path, matches!(item, Item::Dir(_)))?;
        Ok(self.detach(item))
    }

    /// Unhooks an item from the tree and takes its totals off every folder
    /// above it.
    fn detach(&mut self, item: Item) -> Totals {
        let t = &mut self.tree;
        let (parent, total) = match item {
            Item::Dir(id) => {
                let d = &mut t.dirs[id as usize];
                d.removed = true;
                let parent = d.parent.expect("the root is never detached");
                t.dirs[parent as usize].children.retain(|&c| c != id);
                (parent, t.dirs[id as usize].total)
            }
            Item::File(id) => {
                let f = &mut t.files[id as usize];
                f.removed = true;
                let (parent, total) = (f.parent, Totals { size: f.size, alloc: f.alloc, count: 1 });
                t.dirs[parent as usize].files.retain(|&c| c != id);
                (parent, total)
            }
        };
        t.sub_up(parent, total);
        total
    }
}

/// Groups `candidates` (same-size files) by content: first a hash of the
/// first and last 64 KB, then a full BLAKE3 hash of what still matches.
/// `progress(done, total)` counts candidate groups.
pub fn find_duplicates(candidates: Vec<Vec<(Id, PathBuf)>>, cancel: &AtomicBool, progress: impl Fn(usize, usize)) -> Vec<Vec<Id>> {
    let total = candidates.len();
    let mut groups = Vec::new();
    for (done, group) in candidates.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        for same_ends in split_by(group, hash_ends) {
            for same in split_by(same_ends, hash_all) {
                groups.push(same.into_iter().map(|(id, _)| id).collect());
            }
        }
        progress(done + 1, total);
    }
    groups
}

/// Splits files into groups of two or more with the same hash; unreadable
/// files drop out.
fn split_by(files: Vec<(Id, PathBuf)>, hash: impl Fn(&Path) -> io::Result<blake3::Hash>) -> Vec<Vec<(Id, PathBuf)>> {
    let mut by_hash: HashMap<blake3::Hash, Vec<(Id, PathBuf)>> = HashMap::new();
    for (id, path) in files {
        if let Ok(h) = hash(&path) {
            by_hash.entry(h).or_default().push((id, path));
        }
    }
    by_hash.into_values().filter(|g| g.len() > 1).collect()
}

const END: u64 = 64 * 1024;

fn hash_ends(path: &Path) -> io::Result<blake3::Hash> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0; END as usize];
    let n = file.read(&mut buf)?;
    hasher.update(&buf[..n]);
    if len > 2 * END {
        file.seek(SeekFrom::End(-(END as i64)))?;
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
    }
    Ok(hasher.finalize())
}

fn hash_all(path: &Path) -> io::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    io::copy(&mut fs::File::open(path)?, &mut hasher)?;
    Ok(hasher.finalize())
}

#[cfg(target_os = "android")]
fn delete_path(path: &Path, dir: bool) -> io::Result<()> {
    if dir { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}

#[cfg(not(target_os = "android"))]
fn delete_path(path: &Path, _dir: bool) -> io::Result<()> {
    trash::delete(path).map_err(io::Error::other)
}

fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

#[cfg(unix)]
fn inode(meta: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (meta.nlink() > 1).then(|| (meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn inode(_meta: &Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(unix)]
fn allocated(meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}

/// Windows: OneDrive and other cloud-only placeholders use no local space;
/// everything else is rounded up to a 4 KB cluster.
#[cfg(windows)]
fn allocated(meta: &Metadata) -> u64 {
    use std::os::windows::fs::MetadataExt;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x4_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    if meta.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0 {
        0
    } else {
        meta.len().div_ceil(4096) * 4096
    }
}

#[cfg(not(any(unix, windows)))]
fn allocated(meta: &Metadata) -> u64 {
    meta.len()
}

fn modified(meta: &Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// Windows junctions and mounted folders: not followed, like symlinks.
#[cfg(windows)]
fn is_reparse_point(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_meta: &Metadata) -> bool {
    false
}

enum MountKind {
    None,
    /// Another mount of the root's filesystem, such as a btrfs subvolume:
    /// entered, because it holds the same disk's data.
    SameFilesystem,
    /// A different filesystem (another disk, /proc, a network share): skipped.
    Foreign,
}

/// Mount points and their sources, from /proc/self/mountinfo. Empty
/// elsewhere, where reparse points mark mounts instead.
#[derive(Default)]
struct Mounts {
    points: HashMap<PathBuf, String>,
    root_source: String,
}

impl Mounts {
    fn load(root: &Path) -> Self {
        let text = if cfg!(any(target_os = "linux", target_os = "android")) {
            fs::read_to_string("/proc/self/mountinfo").unwrap_or_default()
        } else {
            String::new()
        };
        Self::parse(&text, root)
    }

    fn parse(mountinfo: &str, root: &Path) -> Self {
        let points: HashMap<PathBuf, String> = mountinfo
            .lines()
            .filter_map(|line| {
                let fields: Vec<&str> = line.split(' ').collect();
                let dash = fields.iter().position(|&f| f == "-")?;
                let source = fields.get(dash + 2)?;
                Some((PathBuf::from(unescape(fields.get(4)?)), source.to_string()))
            })
            .collect();
        let root_source = points
            .iter()
            .filter(|(point, _)| root.starts_with(point))
            .max_by_key(|(point, _)| point.as_os_str().len())
            .map(|(_, source)| source.clone())
            .unwrap_or_default();
        Mounts { points, root_source }
    }

    fn classify(&self, path: &Path) -> MountKind {
        match self.points.get(path) {
            None => MountKind::None,
            Some(source) if *source == self.root_source && !source.is_empty() => MountKind::SameFilesystem,
            Some(_) => MountKind::Foreign,
        }
    }
}

/// mountinfo escapes spaces and a few other bytes as octal (`\040`).
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && let Some(code) = field.get(i + 1..i + 4).and_then(|o| u8::from_str_radix(o, 8).ok())
        {
            out.push(code);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("bhandara-index-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            TempTree(root)
        }

        fn file(&self, rel: &str, len: usize) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, vec![b'x'; len]).unwrap();
            path
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const MB: usize = BIG_FILE as usize;

    fn sample(name: &str) -> TempTree {
        let t = TempTree::new(name);
        t.file("videos/a.mkv", 3 * MB);
        t.file("videos/old/b.MKV", 2 * MB);
        t.file("docs/note.txt", 100);
        t.file("docs/deep/more.txt", 50);
        t.file("loose.bin", MB);
        t
    }

    fn scan_all(root: &Path) -> Index {
        scan(root, &[], &AtomicBool::new(false), |_, _| {}).unwrap()
    }

    fn child(index: &Index, parent: Id, name: &str) -> Item {
        index.folder(parent).unwrap().entries.into_iter().find(|e| e.name == name).unwrap().item
    }

    #[test]
    fn totals_roll_up_and_big_files_are_listed() {
        let t = sample("totals_roll_up_and_big_files_are_listed");
        let index = scan_all(&t.0);
        let s = index.summary();
        assert_eq!(s.total.size, (6 * MB + 150) as u64);
        assert_eq!(s.total.count, 5);
        assert_eq!(s.listed_files, 3);
        assert!(!s.partial);

        let root = index.folder(0).unwrap();
        let names: Vec<&str> = root.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["videos", "loose.bin", "docs"]);
        let Item::Dir(docs) = child(&index, 0, "docs") else { panic!() };
        let docs = index.folder(docs).unwrap();
        assert_eq!(docs.total.size, 150);
        assert_eq!(docs.small, Totals { size: 100, alloc: docs.small.alloc, count: 1 });
        assert_eq!(docs.crumbs, [Crumb { id: 0, name: root.name.clone() }]);
        assert_eq!(docs.path, t.0.join("docs"));
    }

    #[test]
    fn types_and_file_filters() {
        let t = sample("types_and_file_filters");
        let index = scan_all(&t.0);
        let types = index.types();
        let mkv = types.iter().find(|s| s.ext == "mkv").unwrap();
        assert_eq!((mkv.total.size, mkv.total.count, mkv.listed), ((5 * MB) as u64, 2, 2));
        let txt = types.iter().find(|s| s.ext == "txt").unwrap();
        assert_eq!((txt.total.size, txt.listed), (150, 0));

        let mkvs = index.files(&FileFilter { ext: Some("MKV".into()), ..Default::default() });
        assert_eq!(mkvs.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["a.mkv", "b.MKV"]);
        assert_eq!(mkvs[1].path, t.0.join("videos/old/b.MKV"));
        let big = index.files(&FileFilter { min_size: 2 * BIG_FILE, limit: 1, ..Default::default() });
        assert_eq!(big.len(), 1);
        let none = index.files(&FileFilter { modified_before: Some(0), ..Default::default() });
        assert!(none.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn hard_links_count_once() {
        let t = sample("hard_links_count_once");
        fs::hard_link(t.0.join("loose.bin"), t.0.join("docs/link.bin")).unwrap();
        let index = scan_all(&t.0);
        assert_eq!(index.summary().total.size, (6 * MB + 150) as u64);
        assert!(index.duplicate_candidates().is_empty(), "a hard link is not a duplicate");
    }

    #[test]
    fn exclusions_and_cancel() {
        let t = sample("exclusions_and_cancel");
        let index = scan(&t.0, &[t.0.join("videos")], &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(index.summary().total.size, (MB + 150) as u64);

        let index = scan(&t.0, &[], &AtomicBool::new(true), |_, _| {}).unwrap();
        assert!(index.summary().partial);
    }

    #[test]
    fn delete_updates_totals_without_a_rescan() {
        let t = sample("delete_updates_totals_without_a_rescan");
        let mut index = scan_all(&t.0);
        let Item::Dir(videos) = child(&index, 0, "videos") else { panic!() };
        let Item::Dir(old) = child(&index, videos, "old") else { panic!() };

        let removed = index
            .delete_with(Item::Dir(old), |path, dir| {
                assert!(dir);
                fs::remove_dir_all(path)
            })
            .unwrap();
        assert_eq!(removed.size, (2 * MB) as u64);
        assert!(!t.0.join("videos/old").exists());
        assert_eq!(index.folder(videos).unwrap().total.size, (3 * MB) as u64);
        assert_eq!(index.summary().total.size, (4 * MB + 150) as u64);
        assert!(!index.is_live(Item::Dir(old)));
        assert!(index.folder(old).is_none(), "stale ids are refused");

        let refused = index.delete_with(Item::Dir(0), |_, _| unreachable!());
        assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn rescan_replaces_one_folder() {
        let t = sample("rescan_replaces_one_folder");
        let mut index = scan_all(&t.0);
        let Item::Dir(docs) = child(&index, 0, "docs") else { panic!() };
        t.file("docs/new.iso", 4 * MB);
        let fresh = index.rescan(docs, &[], &AtomicBool::new(false)).unwrap();
        assert!(!index.is_live(Item::Dir(docs)));
        assert_eq!(index.folder(fresh).unwrap().total.size, (4 * MB + 150) as u64);
        assert_eq!(index.summary().total.size, (10 * MB + 150) as u64);
        let root = index.folder(0).unwrap();
        assert_eq!(root.entries.iter().filter(|e| e.name == "docs").count(), 1, "replaced, not duplicated");
    }

    #[test]
    fn duplicates_match_content_not_just_size() {
        let t = TempTree::new("dupes");
        t.file("a/one.bin", 2 * MB);
        t.file("b/two.bin", 2 * MB);
        let other = t.file("c/three.bin", 2 * MB);
        let mut bytes = fs::read(&other).unwrap();
        *bytes.last_mut().unwrap() = b'y';
        fs::write(&other, bytes).unwrap();

        let index = scan_all(&t.0);
        let candidates = index.duplicate_candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].len(), 3);
        let groups = find_duplicates(candidates, &AtomicBool::new(false), |_, _| {});
        assert_eq!(groups.len(), 1);
        let mut names: Vec<String> = groups[0].iter().map(|&id| index.file(id).unwrap().name).collect();
        names.sort();
        assert_eq!(names, ["one.bin", "two.bin"]);
    }

    #[test]
    fn btrfs_subvolumes_are_entered_and_other_disks_skipped() {
        let mountinfo = "\
29 1 0:26 /@ / rw,relatime shared:1 - btrfs /dev/mapper/root rw
30 29 0:28 /@home /home rw,relatime shared:2 - btrfs /dev/mapper/root rw
31 29 0:5 / /proc rw shared:3 - proc proc rw
32 30 8:17 / /home/u/USB\\040STICK rw - exfat /dev/sdb1 rw";
        let mounts = Mounts::parse(mountinfo, Path::new("/"));
        assert!(matches!(mounts.classify(Path::new("/home")), MountKind::SameFilesystem));
        assert!(matches!(mounts.classify(Path::new("/proc")), MountKind::Foreign));
        assert!(matches!(mounts.classify(Path::new("/home/u/USB STICK")), MountKind::Foreign));
        assert!(matches!(mounts.classify(Path::new("/usr")), MountKind::None));
    }
}
