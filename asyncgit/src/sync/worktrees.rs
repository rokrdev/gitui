use super::{repository::repo, CommitId, RepoPath};
use crate::error::Result;
use git2::{Repository, WorktreeLockStatus};
use std::path::{Path, PathBuf};

/// Information about a registered Git worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeInfo {
	/// Absolute path to the worktree.
	pub path: PathBuf,
	/// Checked-out branch, or `None` for a detached/unborn HEAD.
	pub branch: Option<String>,
	/// Commit currently checked out in the worktree.
	pub head: Option<CommitId>,
	/// Whether this is the worktree `GitUI` currently has open.
	pub is_current: bool,
	/// Whether the worktree is locked against pruning.
	pub is_locked: bool,
	/// Whether the worktree metadata and path are valid.
	pub is_valid: bool,
}

/// Return the main worktree followed by every registered linked worktree.
pub fn get_worktrees(
	repo_path: &RepoPath,
) -> Result<Vec<WorktreeInfo>> {
	let current_repo = repo(repo_path)?;
	let current_path = current_repo.workdir().map(Path::to_path_buf);
	let main_repo = Repository::open(current_repo.commondir())?;
	let mut result = Vec::new();

	if let Some(path) = main_repo.workdir() {
		result.push(worktree_info(
			path.to_path_buf(),
			false,
			true,
			current_path.as_deref(),
		));
	}

	for name in &main_repo.worktrees()? {
		let Some(name) = name? else {
			continue;
		};
		let linked = main_repo.find_worktree(name)?;
		let is_locked = matches!(
			linked.is_locked()?,
			WorktreeLockStatus::Locked(_)
		);
		let is_valid = linked.validate().is_ok();

		result.push(worktree_info(
			linked.path().to_path_buf(),
			is_locked,
			is_valid,
			current_path.as_deref(),
		));
	}

	Ok(result)
}

fn worktree_info(
	path: PathBuf,
	is_locked: bool,
	is_valid: bool,
	current_path: Option<&Path>,
) -> WorktreeInfo {
	let (branch, head) =
		Repository::open(&path).map_or((None, None), |repo| {
			repo.head().map_or((None, None), |head| {
				let branch = head
					.is_branch()
					.then(|| {
						head.shorthand().ok().map(str::to_string)
					})
					.flatten();
				let commit = head
					.peel_to_commit()
					.ok()
					.map(|commit| CommitId::new(commit.id()));

				(branch, commit)
			})
		});

	WorktreeInfo {
		is_current: current_path
			.is_some_and(|current| paths_equal(current, &path)),
		path,
		branch,
		head,
		is_locked,
		is_valid,
	}
}

fn paths_equal(left: &Path, right: &Path) -> bool {
	let left =
		left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
	let right =
		right.canonicalize().unwrap_or_else(|_| right.to_path_buf());

	left == right
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::tests::repo_init;
	use tempfile::TempDir;

	#[test]
	fn lists_main_and_linked_worktrees_from_either_checkout(
	) -> Result<()> {
		let (main_dir, repo) = repo_init()?;
		let linked_parent = TempDir::new()?;
		let linked_path = linked_parent.path().join("linked");
		repo.worktree("linked", &linked_path, None)?;

		let main_repo_path =
			RepoPath::Path(main_dir.path().to_path_buf());
		let from_main = get_worktrees(&main_repo_path)?;

		assert_eq!(from_main.len(), 2);
		assert!(from_main[0].is_current);
		assert!(!from_main[1].is_current);
		assert!(paths_equal(&from_main[1].path, &linked_path));
		assert_eq!(from_main[1].branch.as_deref(), Some("linked"));
		assert!(from_main.iter().all(|worktree| worktree.is_valid));

		let linked_repo_path = RepoPath::Path(linked_path);
		let from_linked = get_worktrees(&linked_repo_path)?;

		assert_eq!(from_linked.len(), 2);
		assert!(!from_linked[0].is_current);
		assert!(from_linked[1].is_current);

		Ok(())
	}
}
