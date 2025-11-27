use crate::repository::git::commit::{get_commits_info, get_current_commit_hash, CommitInfo};
use crate::repository::git::current_branch::get_current_branch;
use ignore::{gitignore::GitignoreBuilder, WalkBuilder};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;

const STATUS_ENTRY_MIN_LENGTH: usize = 4;

#[derive(Clone, Debug)]
pub struct CoreGitaliskRepository {
    pub path: String,
    pub workspace_path: String,
}

/// File information with pre-computed metadata
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// Full file path
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusCode {
    Unmodified,  // ' ' (space)
    Modified,    // M
    TypeChanged, // T
    Added,       // A
    Deleted,     // D
    Renamed,     // R
    Copied,      // C
    Unmerged,    // U
    Untracked,   // ?
    Ignored,     // !
    Unknown,     // For any other cases
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FileStatus {
    pub index: StatusCode,    // X status
    pub worktree: StatusCode, // Y status
}

impl FileStatus {
    pub fn from_status(status: &str) -> Option<Self> {
        if status.len() < 2 {
            return None;
        }

        let index_char = status.chars().nth(0).unwrap();
        let worktree_char = status.chars().nth(1).unwrap();

        Some(Self {
            index: StatusCode::from_char(index_char),
            worktree: StatusCode::from_char(worktree_char),
        })
    }
}

impl StatusCode {
    pub fn from_char(c: char) -> Self {
        match c {
            ' ' => Self::Unmodified,
            'M' => Self::Modified,
            'T' => Self::TypeChanged,
            'A' => Self::Added,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'U' => Self::Unmerged,
            '?' => Self::Untracked,
            '!' => Self::Ignored,
            'X' => Self::Unknown,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StagingStatus {
    Staged,     // Changes in index only
    Unstaged,   // Changes in worktree only
    Both,       // Changes in both index and worktree
    Unmodified, // No changes
    Untracked,  // File not tracked by git
    Conflicted, // Merge conflict
}

impl StagingStatus {
    pub fn from_file_status(status: &FileStatus) -> Self {
        match (status.index, status.worktree) {
            (StatusCode::Untracked, _) | (_, StatusCode::Untracked) => StagingStatus::Untracked,
            (StatusCode::Unmerged, _) | (_, StatusCode::Unmerged) => StagingStatus::Conflicted,
            (StatusCode::Unmodified, StatusCode::Unmodified) => StagingStatus::Unmodified,
            (StatusCode::Unmodified, _) => StagingStatus::Unstaged,
            (_, StatusCode::Unmodified) => StagingStatus::Staged,
            _ => StagingStatus::Both,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileStatusInfo {
    pub path: String,
    pub original_path: Option<String>, // Only present for renamed/copied files
    pub status: FileStatus,
    pub staging: StagingStatus,
}

impl FileStatusInfo {
    fn new(path: String, original_path: Option<String>, status: FileStatus) -> Self {
        // Determine staging status based on index and worktree status
        let staging = StagingStatus::from_file_status(&status);
        Self {
            path,
            original_path,
            status,
            staging,
        }
    }
}

pub struct GetStatusOptions {
    pub include_untracked: bool,
}

impl FileInfo {
    pub fn from_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn extension(&self) -> &str {
        self.path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
    }

    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    }
}

pub struct FileInfoIterator {
    receiver: mpsc::Receiver<Result<FileInfo, std::io::Error>>,
    _walker_thread: thread::JoinHandle<()>,
}

impl Iterator for FileInfoIterator {
    type Item = Result<FileInfo, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}

#[derive(Default)]
pub struct IterFileOptions {
    pub include_ignored: bool,
    pub include_hidden: bool,
    pub exclude_patterns: Vec<String>,
}

impl CoreGitaliskRepository {
    fn join_repo_path(&self, path: &str) -> String {
        Path::join(Path::new(&self.path), Path::new(path))
            .to_string_lossy()
            .to_string()
    }

    pub fn new(path: String, workspace_path: String) -> Self {
        Self {
            path,
            workspace_path,
        }
    }

    pub fn get_current_branch(&self) -> Result<String, std::io::Error> {
        get_current_branch(&self.path)
    }

    pub fn get_current_commit_hash(&self) -> Result<String, std::io::Error> {
        get_current_commit_hash(Path::new(&self.path))
    }

    pub fn get_commits_info(
        &self,
        commit_hashes: &[&str],
    ) -> Result<Vec<CommitInfo>, std::io::Error> {
        get_commits_info(Path::new(&self.path), commit_hashes)
    }

    // TODO: Implement this
    pub fn list_branches(&self) -> Result<Vec<String>, std::io::Error> {
        let branch_names: Vec<String> = vec!["main".to_string()];
        Ok(branch_names)
    }

    pub fn get_status(&self) -> Result<Vec<FileStatusInfo>, std::io::Error> {
        // Validate that the repository path exists, and is sanitized
        let repo_path = Path::new(&self.path);
        if !repo_path.try_exists().unwrap_or(false) {
            return Err(std::io::Error::other("Repository path does not exist"));
        }

        let status = Command::new("git")
            .args(["status", "--porcelain", "-z"])
            .current_dir(repo_path)
            .output()?;

        if !status.status.success() {
            return Err(std::io::Error::other("Failed to get status"));
        }

        // Split by NULL character to handle filenames with newlines
        let status_out = String::from_utf8_lossy(&status.stdout);
        let status_entries: Vec<&str> = status_out.split('\0').filter(|s| !s.is_empty()).collect();

        // Process entries in pairs to detect renames
        let mut processed_indices = std::collections::HashSet::new();
        let mut status_infos: Vec<FileStatusInfo> = status_entries
            .windows(2)
            .enumerate()
            .filter_map(|(i, window)| {
                // Skip if we've already processed this entry as part of a rename
                if processed_indices.contains(&i) {
                    return None;
                }

                // Skip if the entry is too short e.g malformed status
                let (current_entry, next_entry) = (window[0], window[1]);
                if current_entry.len() < STATUS_ENTRY_MIN_LENGTH {
                    return None;
                }

                let status_str = &current_entry[0..2];
                let path_info = &current_entry[3..];

                // For staged renamed/copied files
                if status_str.starts_with('R') || status_str.starts_with('C') {
                    processed_indices.insert(i + 1);
                    return Some(FileStatusInfo::new(
                        self.join_repo_path(path_info),
                        Some(self.join_repo_path(next_entry)),
                        FileStatus::from_status(status_str).unwrap_or(FileStatus {
                            index: StatusCode::Unknown,
                            worktree: StatusCode::Unknown,
                        }),
                    ));
                }

                // For unstaged renames (deleted + untracked pair)
                if status_str == " D"
                    && next_entry.len() >= STATUS_ENTRY_MIN_LENGTH
                    && &next_entry[0..2] == "??"
                {
                    processed_indices.insert(i + 1);
                    return Some(FileStatusInfo::new(
                        self.join_repo_path(&next_entry[3..]),
                        Some(self.join_repo_path(path_info)),
                        FileStatus::from_status(status_str).unwrap_or(FileStatus {
                            index: StatusCode::Unmodified,
                            worktree: StatusCode::Renamed,
                        }),
                    ));
                }

                // Normal file
                Some(FileStatusInfo::new(
                    self.join_repo_path(path_info),
                    None,
                    FileStatus::from_status(status_str).unwrap_or(FileStatus {
                        index: StatusCode::Unknown,
                        worktree: StatusCode::Unknown,
                    }),
                ))
            })
            .collect();

        // Handle the last entry if it wasn't part of a rename
        if let Some(&last_entry) = status_entries.last() {
            if !processed_indices.contains(&(status_entries.len() - 1))
                && last_entry.len() >= STATUS_ENTRY_MIN_LENGTH
            {
                let status_str = &last_entry[0..2];
                let path_info = &last_entry[3..];
                status_infos.push(FileStatusInfo::new(
                    self.join_repo_path(path_info),
                    None,
                    FileStatus::from_status(status_str).unwrap_or(FileStatus {
                        index: StatusCode::Unknown,
                        worktree: StatusCode::Unknown,
                    }),
                ));
            }
        }

        // validate from the file system that all the paths are valid and exist
        status_infos.retain(|status_info| {
            let path = Path::new(&status_info.path);
            path.try_exists().unwrap_or(false)
        });

        Ok(status_infos)
    }

    pub fn iter_repo_files(&self, options: IterFileOptions) -> FileInfoIterator {
        let (sender, receiver) = mpsc::channel();

        let path = self.path.clone();
        // Capture options values to avoid lifetime issues in the closure
        let include_ignored = options.include_ignored;
        let include_hidden = options.include_hidden;
        let exclude_patterns = options.exclude_patterns.clone();

        let walker_thread = thread::spawn(move || {
            // Build custom gitignore matcher from exclude patterns
            let custom_ignores = if !exclude_patterns.is_empty() {
                let mut builder = GitignoreBuilder::new(&path);
                for pattern in &exclude_patterns {
                    if let Err(e) = builder.add_line(None, pattern) {
                        eprintln!(
                            "Warning: Failed to add exclude pattern '{}': {}",
                            pattern, e
                        );
                    }
                }
                builder.build().ok()
            } else {
                None
            };

            let walker = WalkBuilder::new(&path)
                // Enable standard ignore filters when include_ignored is false.
                // When include_ignored=false, we want to respect gitignore, so we enable filters.
                // When include_ignored=true, we want to include ignored files, so we disable filters.
                //
                // This toggles, as a group, all the filters that are enabled by default:
                //
                // - [hidden()](#method.hidden)
                // - [parents()](#method.parents)
                // - [ignore()](#method.ignore)
                // - [git_ignore()](#method.git_ignore)
                // - [git_global()](#method.git_global)
                // - [git_exclude()](#method.git_exclude)
                //
                .standard_filters(!include_ignored)
                // Enables ignoring hidden files.
                // These files include `.gitignore` files, `.DS_store`, etc.
                // By default, we want to include these files to replicate
                // code editors behavior.
                .hidden(!include_hidden)
                // Whether a git repository is required to apply git-related ignore
                // rules (global rules, .gitignore and local exclude rules).
                //
                // When disabled, git-related ignore rules are applied even when searching
                // outside a git repository.
                //
                // We enable this to allow for searching outside of a git repository while
                // still applying git-related ignore rules (which is common in LSP and user
                // code editors).
                .require_git(false)
                // Filter to prevent descending into nested git repositories and .git directories
                .filter_entry(move |entry| {
                    // Skip .git directories and their contents unless explicitly including ignored files
                    if entry.file_name() == ".git" && !include_ignored {
                        return false;
                    }

                    // Skip directories that are nested git repositories
                    // We detect this by checking if a directory (at depth > 0) contains a .git subdirectory
                    if entry.file_type().is_some_and(|ft| ft.is_dir()) && entry.depth() > 0 {
                        let git_dir = entry.path().join(".git");
                        if git_dir.is_dir() {
                            return false;
                        }
                    }

                    // Apply custom exclude patterns using gitignore matching
                    if let Some(ref ignores) = custom_ignores {
                        let entry_path = entry.path();
                        // Strip the base path to get relative path for matching
                        let relative_path = entry_path.strip_prefix(&path).unwrap_or(entry_path);
                        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

                        if let ignore::Match::Ignore(_) = ignores.matched(relative_path, is_dir) {
                            return false;
                        }
                    }

                    true
                })
                .build_parallel();

            walker.run(|| {
                let sender = sender.clone();
                Box::new(move |result| {
                    match result {
                        Ok(entry) => {
                            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                                let file_info = FileInfo::from_path(entry.into_path());
                                if sender.send(Ok(file_info)).is_err() {
                                    return ignore::WalkState::Quit;
                                }
                            }
                        }
                        Err(e) => {
                            let io_error = std::io::Error::other(e.to_string());
                            if sender.send(Err(io_error)).is_err() {
                                return ignore::WalkState::Quit;
                            }
                        }
                    }
                    ignore::WalkState::Continue
                })
            });
        });

        FileInfoIterator {
            receiver,
            _walker_thread: walker_thread,
        }
    }

    pub fn process_repo_files<F>(
        &self,
        mut processor: F,
        options: IterFileOptions,
    ) -> Result<(), std::io::Error>
    where
        F: FnMut(FileInfo) -> Result<(), std::io::Error>,
    {
        for result in self.iter_repo_files(options) {
            match result {
                Ok(file_info) => processor(file_info)?,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    pub fn get_repo_files(
        &self,
        options: IterFileOptions,
    ) -> Result<Vec<FileInfo>, std::io::Error> {
        let mut files = Vec::new();
        self.process_repo_files(
            |file_info| {
                files.push(file_info);
                Ok(())
            },
            options,
        )?;
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_git_repo(path: &std::path::Path) -> std::io::Result<()> {
        let git_dir = path.join(".git");
        fs::create_dir_all(&git_dir)?;
        fs::write(
            git_dir.join("config"),
            "[core]\n\trepositoryformatversion = 0",
        )?;
        Ok(())
    }

    fn create_test_file(path: &std::path::Path, content: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }

    #[test]
    fn test_nested_git_repositories_are_skipped() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();

        create_git_repo(root_path).unwrap();

        create_test_file(&root_path.join("root_file.rs"), "// root file").unwrap();
        create_test_file(&root_path.join("src/main.rs"), "fn main() {}").unwrap();

        let nested_repo = root_path.join("nested_repo");
        fs::create_dir_all(&nested_repo).unwrap();
        create_git_repo(&nested_repo).unwrap();

        create_test_file(&nested_repo.join("nested_file.rs"), "// nested file").unwrap();
        create_test_file(&nested_repo.join("src/lib.rs"), "pub fn test() {}").unwrap();

        let deep_nested = nested_repo.join("deep/another_repo");
        fs::create_dir_all(&deep_nested).unwrap();
        create_git_repo(&deep_nested).unwrap();
        create_test_file(&deep_nested.join("deep_file.rs"), "// deep file").unwrap();

        let regular_dir = root_path.join("regular_dir");
        fs::create_dir_all(&regular_dir).unwrap();
        create_test_file(&regular_dir.join("regular_file.rs"), "// regular file").unwrap();

        let repo = CoreGitaliskRepository::new(
            root_path.to_string_lossy().to_string(),
            temp_dir.path().to_string_lossy().to_string(),
        );

        let files_with_ignored = repo
            .get_repo_files(IterFileOptions {
                include_ignored: true,
                include_hidden: true,
                exclude_patterns: vec![],
            })
            .unwrap();
        let file_paths_with_ignored: Vec<String> = files_with_ignored
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();

        assert!(file_paths_with_ignored
            .iter()
            .any(|p| p.ends_with("root_file.rs")));
        assert!(file_paths_with_ignored
            .iter()
            .any(|p| p.ends_with("src/main.rs")));
        assert!(file_paths_with_ignored
            .iter()
            .any(|p| p.ends_with("regular_dir/regular_file.rs")));

        assert!(!file_paths_with_ignored
            .iter()
            .any(|p| p.ends_with("nested_file.rs")));
        assert!(!file_paths_with_ignored
            .iter()
            .any(|p| p.ends_with("src/lib.rs")));
        assert!(!file_paths_with_ignored
            .iter()
            .any(|p| p.ends_with("deep_file.rs")));

        println!("Found files with ignored: {file_paths_with_ignored:?}");

        assert_eq!(
            files_with_ignored.len(),
            4,
            "Expected exactly 4 files from root, regular directory, and .git/config"
        );

        let files_without_ignored = repo
            .get_repo_files(IterFileOptions {
                include_ignored: false,
                include_hidden: false,
                exclude_patterns: vec![],
            })
            .unwrap();
        let file_paths_without_ignored: Vec<String> = files_without_ignored
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();

        assert!(file_paths_without_ignored
            .iter()
            .any(|p| p.ends_with("root_file.rs")));
        assert!(file_paths_without_ignored
            .iter()
            .any(|p| p.ends_with("src/main.rs")));
        assert!(file_paths_without_ignored
            .iter()
            .any(|p| p.ends_with("regular_dir/regular_file.rs")));

        assert!(!file_paths_without_ignored
            .iter()
            .any(|p| p.contains(".git")));

        println!("Found files without ignored: {file_paths_without_ignored:?}");

        assert_eq!(
            files_without_ignored.len(),
            3,
            "Expected exactly 3 files from root and regular directory, no .git files"
        );
    }

    #[test]
    fn test_single_repository_all_files_included() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();

        create_git_repo(root_path).unwrap();

        create_test_file(&root_path.join("root.rs"), "// root").unwrap();
        create_test_file(&root_path.join("src/main.rs"), "fn main() {}").unwrap();
        create_test_file(&root_path.join("lib/utils.rs"), "pub mod utils;").unwrap();
        create_test_file(
            &root_path.join("tests/integration.rs"),
            "#[test] fn test() {}",
        )
        .unwrap();

        let repo = CoreGitaliskRepository::new(
            root_path.to_string_lossy().to_string(),
            temp_dir.path().to_string_lossy().to_string(),
        );

        let files_with_ignored = repo
            .get_repo_files(IterFileOptions {
                include_ignored: true,
                include_hidden: true,
                exclude_patterns: vec![],
            })
            .unwrap();

        assert_eq!(
            files_with_ignored.len(),
            5,
            "Expected all 5 files to be included (4 source files + .git/config)"
        );

        let file_names_with_ignored: Vec<String> = files_with_ignored
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(file_names_with_ignored.contains(&"root.rs".to_string()));
        assert!(file_names_with_ignored.contains(&"main.rs".to_string()));
        assert!(file_names_with_ignored.contains(&"utils.rs".to_string()));
        assert!(file_names_with_ignored.contains(&"integration.rs".to_string()));
        assert!(file_names_with_ignored.contains(&"config".to_string())); // .git/config

        let files_without_ignored = repo
            .get_repo_files(IterFileOptions {
                include_ignored: false,
                include_hidden: false,
                exclude_patterns: vec![],
            })
            .unwrap();

        assert_eq!(
            files_without_ignored.len(),
            4,
            "Expected 4 source files, no .git files"
        );

        let file_names_without_ignored: Vec<String> = files_without_ignored
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(file_names_without_ignored.contains(&"root.rs".to_string()));
        assert!(file_names_without_ignored.contains(&"main.rs".to_string()));
        assert!(file_names_without_ignored.contains(&"utils.rs".to_string()));
        assert!(file_names_without_ignored.contains(&"integration.rs".to_string()));
        assert!(!file_names_without_ignored.contains(&"config".to_string())); // Should not include .git/config
    }

    #[test]
    fn test_deeply_nested_git_repositories() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();

        create_git_repo(root_path).unwrap();
        create_test_file(&root_path.join("root.rs"), "// root").unwrap();

        let level1 = root_path.join("level1");
        fs::create_dir_all(&level1).unwrap();
        create_git_repo(&level1).unwrap();
        create_test_file(&level1.join("level1.rs"), "// level1").unwrap();

        let level2 = level1.join("level2");
        fs::create_dir_all(&level2).unwrap();
        create_git_repo(&level2).unwrap();
        create_test_file(&level2.join("level2.rs"), "// level2").unwrap();

        let level3 = level2.join("level3");
        fs::create_dir_all(&level3).unwrap();
        create_git_repo(&level3).unwrap();
        create_test_file(&level3.join("level3.rs"), "// level3").unwrap();

        let repo = CoreGitaliskRepository::new(
            root_path.to_string_lossy().to_string(),
            temp_dir.path().to_string_lossy().to_string(),
        );

        let files_with_ignored = repo
            .get_repo_files(IterFileOptions {
                include_ignored: true,
                include_hidden: true,
                exclude_patterns: vec![],
            })
            .unwrap();

        assert_eq!(
            files_with_ignored.len(),
            2,
            "Expected root file and .git/config, nested repos should be skipped"
        );

        let file_names_with_ignored: Vec<String> = files_with_ignored
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(file_names_with_ignored.contains(&"root.rs".to_string()));
        assert!(file_names_with_ignored.contains(&"config".to_string())); // .git/config

        let files_without_ignored = repo
            .get_repo_files(IterFileOptions {
                include_ignored: false,
                include_hidden: false,
                exclude_patterns: vec![],
            })
            .unwrap();

        assert_eq!(
            files_without_ignored.len(),
            1,
            "Expected only root file, no .git files or nested repos"
        );

        let file_name = files_without_ignored[0]
            .path
            .file_name()
            .unwrap()
            .to_string_lossy();
        assert_eq!(file_name, "root.rs");
    }

    #[test]
    fn test_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();

        create_git_repo(root_path).unwrap();

        create_test_file(&root_path.join("include.rs"), "// include").unwrap();
        create_test_file(&root_path.join("exclude.rs"), "// exclude").unwrap();
        create_test_file(&root_path.join("src/main.rs"), "fn main() {}").unwrap();
        create_test_file(&root_path.join("tests/test.rs"), "#[test] fn test() {}").unwrap();

        let repo = CoreGitaliskRepository::new(
            root_path.to_string_lossy().to_string(),
            temp_dir.path().to_string_lossy().to_string(),
        );

        // Test excluding multiple patterns
        let files = repo
            .get_repo_files(IterFileOptions {
                include_ignored: false,
                include_hidden: false,
                exclude_patterns: vec!["tests/".to_string(), "exclude.rs".to_string()],
            })
            .unwrap();

        let file_names: Vec<String> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(file_names.contains(&"include.rs".to_string()));
        assert!(!file_names.contains(&"exclude.rs".to_string()));
        assert!(file_names.contains(&"main.rs".to_string()));
        assert!(!file_names.contains(&"test.rs".to_string()));
    }

    #[test]
    fn test_exclude_patterns_with_globs() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();

        create_git_repo(root_path).unwrap();

        create_test_file(&root_path.join("file.rs"), "// file").unwrap();
        create_test_file(&root_path.join("file.test.rs"), "// test").unwrap();
        create_test_file(&root_path.join("file.spec.rs"), "// spec").unwrap();
        create_test_file(&root_path.join("src/lib.rs"), "// lib").unwrap();
        create_test_file(&root_path.join("src/lib.test.rs"), "// lib test").unwrap();

        let repo = CoreGitaliskRepository::new(
            root_path.to_string_lossy().to_string(),
            temp_dir.path().to_string_lossy().to_string(),
        );

        // Test glob pattern to exclude all test files
        let files = repo
            .get_repo_files(IterFileOptions {
                include_ignored: false,
                include_hidden: false,
                exclude_patterns: vec!["*.test.rs".to_string(), "*.spec.rs".to_string()],
            })
            .unwrap();

        let file_names: Vec<String> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(file_names.contains(&"file.rs".to_string()));
        assert!(!file_names.contains(&"file.test.rs".to_string()));
        assert!(!file_names.contains(&"file.spec.rs".to_string()));
        assert!(file_names.contains(&"lib.rs".to_string()));
        assert!(!file_names.contains(&"lib.test.rs".to_string()));
    }
}
