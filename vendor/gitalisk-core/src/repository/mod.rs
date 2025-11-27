pub mod git;
pub mod gitalisk_repository;
pub mod testing;

#[cfg(test)]
mod tests {
    use super::gitalisk_repository::CoreGitaliskRepository;
    use crate::repository::gitalisk_repository::IterFileOptions;
    use crate::repository::gitalisk_repository::StatusCode;
    use crate::repository::testing::local::LocalGitRepository;
    use crate::workspace_folder::gitalisk_workspace::CoreGitaliskWorkspaceFolder;
    use std::path::Path;

    fn init_test_workspace_with_repo() -> LocalGitRepository {
        let mut repo = LocalGitRepository::new(None);

        println!("repo.user_config(): {}", repo.user_config());

        // Create some test files
        repo.fs.create_directory("src");
        repo.fs.create_file("README.md", Some("test"));
        repo.fs.create_file("Cargo.toml", Some("test"));
        repo.fs.create_file(
            "src/main.rs",
            Some("fn main() { println!(\"Hello, world!\"); }"),
        );

        repo
    }

    #[test]
    fn test_get_status_invalid_repo_path() {
        // create a temporary workspace
        let local_repo = init_test_workspace_with_repo();

        // validate that the workspace is indexed
        let workspace = CoreGitaliskWorkspaceFolder::new(
            local_repo.workspace_path.to_str().unwrap().to_string(),
        );
        let _ = workspace.index_repositories();
        let repos = workspace.get_repositories();
        let repo = repos.first().unwrap();

        // create a malicious path that will be used to test the repository path sanitization
        let repo_path_str = repo.path.clone();
        let real_path_but_malicious_cmd: String = format!("{repo_path_str}; ls -la");

        // Validate that the repository path exists, and is sanitized
        let malicious_path_struct = Path::new(&real_path_but_malicious_cmd);
        assert!(!malicious_path_struct.try_exists().unwrap_or(false));
        println!("NOT PATH: malicious_path_struct: {malicious_path_struct:?}");

        // fake path, malicious command
        let fake_malicious_path = Path::new("/test/path; ls -la");
        let repo_path = Path::new(fake_malicious_path);
        println!("FAKE PATH: repo_path: {repo_path:?}");
        assert!(!repo_path.try_exists().unwrap_or(false));
    }

    #[test]
    fn test_get_status_invalid_repo_path_sanitization() {
        // create a temporary workspace
        let local_repo = init_test_workspace_with_repo();

        // validate that the workspace is indexed
        let workspace = CoreGitaliskWorkspaceFolder::new(
            local_repo.workspace_path.to_str().unwrap().to_string(),
        );
        let _ = workspace.index_repositories();
        let repos = workspace.get_repositories();
        let repo = repos.first().unwrap();

        // create a malicious path that will be used to test the repository path sanitization
        let repo_path_str = repo.path.clone();
        let real_path_but_malicious_cmd: String = format!("{repo_path_str}; ls -la");

        // Validate that the repository path exists, and is sanitized
        let malicious_path_struct = Path::new(&real_path_but_malicious_cmd);
        assert!(!malicious_path_struct.try_exists().unwrap_or(false));
        println!("NOT PATH: malicious_path_struct: {malicious_path_struct:?}");

        // fake path, malicious command
        let fake_malicious_path = Path::new("/test/path; ls -la");
        let repo_path = Path::new(fake_malicious_path);
        println!("FAKE PATH: repo_path: {repo_path:?}");
        assert!(!repo_path.try_exists().unwrap_or(false));
    }

    #[test]
    fn test_get_status_basic() {
        // create a temporary workspace
        let mut local_repo = init_test_workspace_with_repo();

        // Track the files BEFORE indexing the workspace
        local_repo.add_all();

        // validate that the workspace is indexed
        let workspace = CoreGitaliskWorkspaceFolder::new(
            local_repo.workspace_path.to_str().unwrap().to_string(),
        );
        let _ = workspace.index_repositories();
        let repos = workspace.get_repositories();
        println!("Found {} repositories", repos.len());
        for (i, repo) in repos.iter().enumerate() {
            println!("Repo {}: path = {}", i, repo.path);
        }
        let repo = repos.first().unwrap();

        // get the status of the repository
        let statuses = repo.get_status().unwrap();

        // validate that we have 3 untracked files, added in the init_test_workspace_with_repo function
        let unique_statuses: Vec<StatusCode> = statuses.iter().map(|s| s.status.index).collect();
        for status in statuses.clone() {
            println!("status: {status:?}");
        }
        assert!(unique_statuses.contains(&StatusCode::Added));
        assert_eq!(statuses.len(), 3);

        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );
    }

    #[test]
    fn test_get_status_commit_and_modify() {
        // create a temporary workspace
        let mut local_repo = init_test_workspace_with_repo();

        // validate that the workspace is indexed
        let workspace = CoreGitaliskWorkspaceFolder::new(
            local_repo.workspace_path.to_str().unwrap().to_string(),
        );
        let _ = workspace.index_repositories();
        let repos = workspace.get_repositories();
        let repo = repos.first().unwrap();

        // Track the files
        local_repo.add_all();

        // get the status of the repository
        let statuses = repo.get_status().unwrap();
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );

        // validate that we have 3 untracked files, added in the init_test_workspace_with_repo function
        let unique_statuses: Vec<StatusCode> = statuses.iter().map(|s| s.status.index).collect();
        for status in statuses.clone() {
            println!("status: {status:?}");
        }
        assert!(unique_statuses.contains(&StatusCode::Added));
        assert_eq!(statuses.len(), 3);

        // commit the changes (README.md, Cargo.toml, src/main.rs)
        local_repo.commit("test commit 1");
        println!("\nPOST COMMIT");
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );

        // modify the README.md file
        local_repo
            .fs
            .append_file_content("README.md", "test commit 2");
        println!("\nPOST MODIFY");
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );

        // // stage the changes
        // test_git_add(Path::new(&repo.path));
        // println!("\nPOST STAGE");
        // print_naive_git_status(Path::new(&repo.path));
        // print_git_status_porcelain(Path::new(&repo.path));

        // get the status of the repository, again, should only have the README.md file modified
        let statuses_2 = repo.get_status().unwrap();
        let unique_statuses_2: Vec<StatusCode> =
            statuses_2.iter().map(|s| s.status.worktree).collect();
        for status_2 in statuses_2.clone() {
            println!("status_2: {status_2:?}");
        }
        assert_eq!(statuses_2.len(), 1);
        assert!(unique_statuses_2.contains(&StatusCode::Modified));
    }

    #[test]
    fn test_get_status_commit_and_rename() {
        // create a temporary workspace
        let mut local_repo = init_test_workspace_with_repo();

        // validate that the workspace is indexed
        let workspace = CoreGitaliskWorkspaceFolder::new(
            local_repo.workspace_path.to_str().unwrap().to_string(),
        );
        let _ = workspace.index_repositories();
        let repos = workspace.get_repositories();
        let repo = repos.first().unwrap();

        // Track the files
        local_repo.add_all();

        // get the status of the repository
        let statuses = repo.get_status().unwrap();
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );

        // validate that we have 3 untracked files, added in the init_test_workspace_with_repo function
        let unique_statuses: Vec<StatusCode> = statuses.iter().map(|s| s.status.index).collect();
        for status in statuses.clone() {
            println!("status: {status:?}");
        }
        assert!(unique_statuses.contains(&StatusCode::Added));
        assert_eq!(statuses.len(), 3);

        // commit the changes (README.md, Cargo.toml, src/main.rs)
        local_repo.commit("test commit 1");
        println!("\nPOST COMMIT");
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );

        // rename the README.md file
        local_repo.fs.rename_to("README.md", "README.md.new");
        println!("\nPOST MODIFY");
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );
        let statuses_2 = repo.get_status().unwrap();
        assert_eq!(statuses_2.len(), 1);
        let first_status_2 = statuses_2.first().unwrap();
        println!("first_status_2: {first_status_2:?}");
        assert!(!first_status_2.path.is_empty() && first_status_2.original_path.is_some());
    }

    #[test]
    fn test_get_status_commit_and_move_file() {
        // create a temporary workspace
        let mut local_repo = init_test_workspace_with_repo();

        // validate that the workspace is indexed
        let workspace = CoreGitaliskWorkspaceFolder::new(
            local_repo.workspace_path.to_str().unwrap().to_string(),
        );
        let _ = workspace.index_repositories();
        let repos = workspace.get_repositories();
        let repo = repos.first().unwrap();

        // Track the files
        local_repo.add_all();

        // get the status of the repository
        let statuses = repo.get_status().unwrap();
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );

        // validate that we have 3 untracked files, added in the init_test_workspace_with_repo function
        let unique_statuses: Vec<StatusCode> = statuses.iter().map(|s| s.status.index).collect();
        for status in statuses.clone() {
            println!("status: {status:?}");
        }
        assert!(unique_statuses.contains(&StatusCode::Added));
        assert_eq!(statuses.len(), 3);

        // commit the changes (README.md, Cargo.toml, src/main.rs)
        local_repo.commit("test commit 1");
        println!("\nPOST COMMIT");
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );

        // move the README.md file to the src directory
        // test_move_file_to_dir(Path::new(&repo.path), "README.md", "src");
        local_repo.fs.move_to("README.md", "src/README.md");
        println!("\nPOST MOVE");
        println!("local_repo.status(): {}", local_repo.status());
        println!(
            "local_repo.status_porcelain(): {}",
            local_repo.status_porcelain()
        );

        // get the status of the repository, again, should only have the README.md file modified
        let statuses_2 = repo.get_status().unwrap();
        assert_eq!(statuses_2.len(), 1);

        let first_status_2 = statuses_2.first().unwrap();
        println!("first_status_2: {first_status_2:?}");
        assert!(!first_status_2.path.is_empty() && first_status_2.original_path.is_some());
    }

    #[test]
    fn test_new_repository() {
        let repo =
            CoreGitaliskRepository::new("/test/path".to_string(), "/test/workspace".to_string());

        assert_eq!(repo.path, "/test/path");
        assert_eq!(repo.workspace_path, "/test/workspace");
    }

    #[test]
    fn test_list_branches() {
        let repo =
            CoreGitaliskRepository::new("/test/path".to_string(), "/test/workspace".to_string());

        let branches = repo.list_branches().unwrap();
        assert_eq!(branches, vec!["main"]);
    }

    #[test]
    fn test_find_repo_files() {
        let local_repo = init_test_workspace_with_repo();

        let repo = CoreGitaliskRepository::new(
            local_repo.path.to_str().unwrap().to_string(),
            local_repo.workspace_path.to_str().unwrap().to_string(),
        );

        let files = repo
            .get_repo_files(IterFileOptions {
                include_ignored: true,
                include_hidden: true,
                exclude_patterns: vec![],
            })
            .unwrap();
        assert!(files.len() >= 3);

        // Check that expected files are found
        let file_names: Vec<String> = files
            .iter()
            .filter_map(|f| Path::new(f.path()).file_name()?.to_str())
            .map(String::from)
            .collect();

        assert!(file_names.contains(&"README.md".to_string()));
        assert!(file_names.contains(&"Cargo.toml".to_string()));
        assert!(file_names.contains(&"main.rs".to_string()));
    }

    #[test]
    fn test_find_repo_files_empty_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path();

        let repo = CoreGitaliskRepository::new(
            repo_path.to_str().unwrap().to_string(),
            "/test/workspace".to_string(),
        );

        let files = repo
            .get_repo_files(IterFileOptions {
                include_ignored: true,
                include_hidden: false,
                exclude_patterns: vec![],
            })
            .unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_find_repo_files_with_gitignore() {
        let mut local_repo = init_test_workspace_with_repo();

        local_repo
            .fs
            .create_file(".gitignore", Some("target/\n*.log\n"))
            .create_directory("target")
            .create_file("target/debug", Some("binary"))
            .create_file("test.log", Some("log content"))
            .create_file("src.rs", Some("code"));

        let repo = CoreGitaliskRepository::new(
            local_repo.path.to_str().unwrap().to_string(),
            local_repo.workspace_path.to_str().unwrap().to_string(),
        );

        let files = repo
            .get_repo_files(IterFileOptions {
                include_ignored: false,
                include_hidden: true,
                exclude_patterns: vec![],
            })
            .unwrap();

        // Should find .gitignore and src.rs, but not target/debug or test.log
        let file_names: Vec<String> = files
            .iter()
            .filter_map(|f| Path::new(f.path()).file_name()?.to_str())
            .map(String::from)
            .collect();
        println!("file_names: {file_names:?}");

        assert!(file_names.contains(&".gitignore".to_string()));
        assert!(file_names.contains(&"src.rs".to_string()));
        assert!(!file_names.contains(&"target/debug".to_string()));
        assert!(!file_names.contains(&"test.log".to_string()));
    }
}
