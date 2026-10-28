use serde::Serialize;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Folder {
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct FolderUsage {
    pub total: u64,
    pub folders: Vec<Folder>,
}

/// Apparent size of `path`, staying on its filesystem (like `du -x -b`).
/// Hard-linked files count once, as in `du`. Unreadable entries are skipped.
fn size_of(path: &Path) -> u64 {
    #[cfg(unix)]
    let mut seen = std::collections::HashSet::new();
    WalkDir::new(path)
        .same_file_system(true)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|meta| !meta.is_dir())
        .filter(|_meta| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if _meta.nlink() > 1 {
                    return seen.insert((_meta.dev(), _meta.ino()));
                }
            }
            true
        })
        .map(|meta| meta.len())
        .sum()
}

/// The `limit` largest direct children of `dir`, biggest first. Each child
/// is measured on its own thread.
pub fn largest_children(dir: &Path, limit: usize) -> std::io::Result<FolderUsage> {
    let children: Vec<PathBuf> = std::fs::read_dir(dir)?.filter_map(Result::ok).map(|e| e.path()).collect();

    let mut folders: Vec<Folder> = std::thread::scope(|scope| {
        let handles: Vec<_> = children
            .iter()
            .map(|path| scope.spawn(move || Folder {
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                bytes: if path.is_symlink() { 0 } else { size_of(path) },
                path: path.clone(),
            }))
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });

    let total = folders.iter().map(|f| f.bytes).sum();
    folders.retain(|f| f.path.is_dir() && !f.path.is_symlink());
    folders.sort_by_key(|f| std::cmp::Reverse(f.bytes));
    folders.truncate(limit);
    Ok(FolderUsage { total, folders })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn ranks_children_by_size() {
        let root = std::env::temp_dir().join(format!("bhandara-folders-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("small")).unwrap();
        fs::create_dir_all(root.join("big/nested")).unwrap();
        fs::write(root.join("small/a"), vec![0; 10]).unwrap();
        fs::write(root.join("big/nested/b"), vec![0; 300]).unwrap();
        fs::write(root.join("loose"), vec![0; 5]).unwrap();

        let usage = largest_children(&root, 5).unwrap();
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(usage.total, 315);
        let names: Vec<&str> = usage.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["big", "small"]);
        assert_eq!(usage.folders[0].bytes, 300);
    }
}
