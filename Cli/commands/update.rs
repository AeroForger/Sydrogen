use std::path::Path;
use std::process::{Command, ExitCode};
use which::which;

const FURNACE_REPO: &str = "https://github.com/AeroForger/Sydrogen.git";

pub fn update_furnace() -> ExitCode {
    match update_furnace_internal(FURNACE_REPO) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: Failed to update furnace: {}", e);
            ExitCode::FAILURE
        }
    }
}

pub fn update_furnace_internal(repo_url: &str) -> Result<(), String> {
    check_dependency("git")?;
    check_dependency("cargo")?;
    let temp_dir = tempfile::TempDir::new().map_err(|e| e.to_string())?;
    let temp_path = temp_dir.path();
    println!("Downloading source...");
    run_git_cmd(temp_path, &["clone", repo_url, "."])
        .map_err(|e| format!("Error: Failed to clone repository: {}", e))?;
    let latest_tag = get_latest_git_tag(temp_path)?;
    println!("Latest tag: {}", &latest_tag);
    run_git_cmd(temp_path, &["checkout", &latest_tag])
        .map_err(|_| "Error: Failed to execute git checkout".to_string())?;
    println!("Installing Furnace...");
    let install_status = Command::new("cargo")
        .args(["install", "--path", ".", "--locked"])
        .current_dir(temp_path)
        .status()
        .map_err(|e| e.to_string())?;

    if !install_status.success() {
        eprintln!("Error: `cargo install --path .` execution failed");
        return Err("Error: `cargo install --path .` execution failed".to_string());
    }
    println!("Furnace updated successfully.");
    Ok(())
}

fn check_dependency(cmd: &str) -> Result<(), String> {
    which(cmd).map_err(|_| format!("`{}` is unavailable. Please install {} first.", cmd, cmd))?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AlphaVersion {
    major: u32,
    stage: u8,
    minor: u32,
}

fn parse_alpha_tag(tag: &str) -> Option<AlphaVersion> {
    let version = tag.strip_prefix("alpha-")?;

    // alpha-N-half-M
    if let Some((major, half)) = version.split_once("-half-") {
        return Some(AlphaVersion {
            major: major.parse().ok()?,
            stage: 0, // prerelease, before alpha-N
            minor: half.parse().ok()?,
        });
    }

    // alpha-N.M
    if let Some((major, minor)) = version.split_once('.') {
        return Some(AlphaVersion {
            major: major.parse().ok()?,
            stage: 2, // after alpha-N
            minor: minor.parse().ok()?,
        });
    }

    // alpha-N
    Some(AlphaVersion {
        major: version.parse().ok()?,
        stage: 1,
        minor: 0,
    })
}

fn get_latest_git_tag(dir: &Path) -> Result<String, String> {
    run_git_cmd(dir, &["fetch", "--tags"])?;

    let tags = run_git_cmd(dir, &["tag", "-l", "alpha-*"])?;

    tags.lines()
        .filter_map(|tag| parse_alpha_tag(tag).map(|version| (version, tag.to_string())))
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, tag)| tag)
        .ok_or_else(|| "No valid Sydrogen alpha tags found".to_string())
}

fn run_git_cmd<P: AsRef<Path>>(working_dir: P, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(working_dir.as_ref())
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|err| format!("Failed to execute git command: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("Command `git {}` exited with error", args.join(" "))
        } else {
            stderr
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = dir.path();
        run_git_cmd(path, &["init"]).unwrap();
        run_git_cmd(path, &["config", "user.name", "Test"]).unwrap();
        run_git_cmd(path, &["config", "user.email", "test@example.com"]).unwrap();
        let cargo_toml = r#"
            [package]
            name = "furnace-mock"
            version = "0.1.0"
            edition = "2021"

            [[bin]]
            name = "furnace-mock"
            path = "src/main.rs"
        "#;
        std::fs::write(path.join("Cargo.toml"), cargo_toml).unwrap();
        std::fs::create_dir_all(path.join("src")).unwrap();
        std::fs::write(
            path.join("src/main.rs"),
            "fn main() { println!(\"mock\"); }",
        )
        .unwrap();
        run_git_cmd(path, &["add", "."]).unwrap();
        run_git_cmd(path, &["commit", "-m", "initial"]).unwrap();
        dir
    }

    #[test]
    fn test_check_dependency() {
        let tool_name = "Idontfuckexist";
        assert!(check_dependency(tool_name).is_err());
        let tool_name = "cargo";
        assert!(check_dependency(tool_name).is_ok())
    }

    #[test]
    fn test_get_latest_git_tag_success() {
        let repo_dir = create_mock_git_repo();
        let path = repo_dir.path();

        run_git_cmd(path, &["tag", "v0.1.0"]).unwrap();
        run_git_cmd(path, &["tag", "v0.3.2"]).unwrap();
        run_git_cmd(path, &["tag", "alpha-4"]).unwrap();
        run_git_cmd(path, &["tag", "alpha-5-half-1"]).unwrap();
        run_git_cmd(path, &["tag", "alpha-5"]).unwrap();
        run_git_cmd(path, &["tag", "alpha-5.1"]).unwrap();
        run_git_cmd(path, &["tag", "alpha-7"]).unwrap();

        let tag = get_latest_git_tag(path).unwrap();

        assert_eq!(tag, "alpha-7");
    }

    #[test]
    fn test_get_latest_git_tag_no_tags_error() {
        let repo_dir = create_mock_git_repo();
        let path = repo_dir.path();
        let result = get_latest_git_tag(path);

        assert!(result
            .unwrap_err()
            .contains("No valid Sydrogen alpha tags found"));
    }
    #[test]
    fn test_alpha_version_ordering() {
        assert!(parse_alpha_tag("alpha-4").unwrap() < parse_alpha_tag("alpha-5-half-1").unwrap());

        assert!(parse_alpha_tag("alpha-5-half-1").unwrap() < parse_alpha_tag("alpha-5").unwrap());

        assert!(parse_alpha_tag("alpha-5").unwrap() < parse_alpha_tag("alpha-5.1").unwrap());

        assert!(parse_alpha_tag("alpha-5.1").unwrap() < parse_alpha_tag("alpha-7").unwrap());

        assert!(parse_alpha_tag("v0.3.2").is_none());
    }

    #[test]
    fn test_get_latest_git_tag_invalid_dir() {
        let empty_dir = tempfile::tempdir().unwrap();
        let result = get_latest_git_tag(empty_dir.path());
        assert!(result.unwrap_err().contains("not a git repository"));
    }

    #[ignore = "This will install real mock binary to your path"]
    #[test]
    fn test_update_furnace_success() {
        let mock_repo = create_mock_git_repo();
        let mock_repo_url = mock_repo.path().to_str().unwrap();
        run_git_cmd(mock_repo_url, &["tag", "alpha-7"]).unwrap();
        let result = update_furnace_internal(mock_repo_url);
        assert!(result.is_ok())
    }

    #[test]
    fn test_update_furnace_invalid_repo_failure() {
        let invalid_repo_url = "nonexistent_org_123/nonexistent_repo_456.git";
        let result = update_furnace_internal(invalid_repo_url);
        assert!(result.unwrap_err().contains("does not exist"))
    }
}
