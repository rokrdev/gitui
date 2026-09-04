use crate::{
	app::Environment,
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState, ScrollType,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, Queue},
	strings,
	ui::{self, style::SharedTheme, Orientation},
};
use anyhow::Result;
use asyncgit::sync::{
	get_worktrees, RepoPath, RepoPathRef, WorktreeInfo,
};
use crossterm::event::Event;
use ratatui::{
	layout::{Constraint, Margin, Rect},
	text::Span,
	widgets::{Block, Borders, Cell, Row, Table, TableState},
	Frame,
};
use std::cell::Cell as StateCell;

/// Browse all worktrees registered for the current repository.
pub struct Worktrees {
	repo: RepoPathRef,
	entries: Vec<WorktreeInfo>,
	visible: bool,
	table_state: StateCell<TableState>,
	current_height: StateCell<usize>,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
}

impl Worktrees {
	pub fn new(env: &Environment) -> Self {
		Self {
			repo: env.repo.clone(),
			entries: Vec::new(),
			visible: false,
			table_state: StateCell::new(TableState::default()),
			current_height: StateCell::new(0),
			queue: env.queue.clone(),
			theme: env.theme.clone(),
			key_config: env.key_config.clone(),
		}
	}

	pub fn update(&mut self) -> Result<()> {
		if self.visible {
			self.entries = get_worktrees(&self.repo.borrow())?;
			let selection = self
				.table_state
				.get_mut()
				.selected()
				.unwrap_or_default();
			let max = self.entries.len().saturating_sub(1);
			self.table_state.get_mut().select(
				if self.entries.is_empty() {
					None
				} else {
					Some(selection.min(max))
				},
			);
		}

		Ok(())
	}

	fn selected(&self) -> Option<&WorktreeInfo> {
		let table_state = self.table_state.take();
		let selected = table_state
			.selected()
			.and_then(|index| self.entries.get(index));
		self.table_state.set(table_state);

		selected
	}

	fn can_open_selected(&self) -> bool {
		self.selected().is_some_and(|worktree| {
			worktree.is_valid && !worktree.is_current
		})
	}

	fn open_selected(&self) {
		if let Some(worktree) = self.selected().filter(|worktree| {
			worktree.is_valid && !worktree.is_current
		}) {
			self.queue.push(InternalEvent::OpenRepoPath {
				repo: RepoPath::Path(worktree.path.clone()),
			});
		}
	}

	fn move_selection(&self, scroll_type: ScrollType) -> bool {
		if self.entries.is_empty() {
			return false;
		}

		let mut table_state = self.table_state.take();
		let old = table_state.selected().unwrap_or_default();
		let max = self.entries.len().saturating_sub(1);
		let page = self.current_height.get().saturating_sub(2).max(1);
		let new = match scroll_type {
			ScrollType::Up => old.saturating_sub(1),
			ScrollType::Down => old.saturating_add(1).min(max),
			ScrollType::PageUp => old.saturating_sub(page),
			ScrollType::PageDown => old.saturating_add(page).min(max),
			ScrollType::Home => 0,
			ScrollType::End => max,
		};

		table_state.select(Some(new));
		self.table_state.set(table_state);

		new != old
	}

	fn rows(&self) -> impl Iterator<Item = Row<'_>> {
		self.entries.iter().map(|worktree| {
			let marker = if worktree.is_current { "*" } else { " " };
			let branch =
				worktree.branch.as_deref().unwrap_or("(detached)");
			let head = worktree.head.as_ref().map_or_else(
				|| "-------".to_string(),
				asyncgit::sync::CommitId::get_short_string,
			);
			let state = if !worktree.is_valid {
				"missing"
			} else if worktree.is_locked {
				"locked"
			} else if worktree.is_current {
				"current"
			} else {
				""
			};

			Row::new(vec![
				Cell::from(marker)
					.style(self.theme.text(true, false)),
				Cell::from(worktree.path.to_string_lossy())
					.style(self.theme.text(true, false)),
				Cell::from(branch)
					.style(self.theme.text(true, false)),
				Cell::from(head).style(self.theme.commit_hash(false)),
				Cell::from(state)
					.style(self.theme.text(false, false)),
			])
		})
	}
}

impl DrawableComponent for Worktrees {
	fn draw(&self, f: &mut Frame, rect: Rect) -> Result<()> {
		if !self.visible {
			return Ok(());
		}

		let header =
			Row::new(["", "Path", "Branch", "HEAD", "State"])
				.style(self.theme.title(true));
		let table = Table::new(
			self.rows(),
			[
				Constraint::Length(1),
				Constraint::Percentage(55),
				Constraint::Percentage(25),
				Constraint::Length(7),
				Constraint::Length(8),
			],
		)
		.header(header)
		.column_spacing(1)
		.row_highlight_style(self.theme.text(true, true))
		.block(
			Block::default()
				.title(Span::styled(
					"Worktrees",
					self.theme.title(true),
				))
				.borders(Borders::ALL)
				.border_style(self.theme.block(true)),
		);

		let mut table_state = self.table_state.take();
		f.render_stateful_widget(table, rect, &mut table_state);

		let scrollbar_area = rect.inner(Margin {
			vertical: 1,
			horizontal: 0,
		});
		ui::draw_scrollbar(
			f,
			scrollbar_area,
			&self.theme,
			self.entries.len(),
			table_state.selected().unwrap_or_default(),
			Orientation::Vertical,
		);
		self.current_height.set(scrollbar_area.height.into());
		self.table_state.set(table_state);

		Ok(())
	}
}

impl Component for Worktrees {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.visible || force_all {
			out.push(CommandInfo::new(
				strings::commands::scroll(&self.key_config),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::open_worktree(&self.key_config),
				self.can_open_selected(),
				true,
			));
		}

		visibility_blocking(self)
	}

	fn event(&mut self, event: &Event) -> Result<EventState> {
		if !self.visible {
			return Ok(EventState::NotConsumed);
		}

		if let Event::Key(key) = event {
			if key_match(key, self.key_config.keys.move_up) {
				self.move_selection(ScrollType::Up);
				return Ok(EventState::Consumed);
			} else if key_match(key, self.key_config.keys.move_down) {
				self.move_selection(ScrollType::Down);
				return Ok(EventState::Consumed);
			} else if key_match(key, self.key_config.keys.page_up) {
				self.move_selection(ScrollType::PageUp);
				return Ok(EventState::Consumed);
			} else if key_match(key, self.key_config.keys.page_down) {
				self.move_selection(ScrollType::PageDown);
				return Ok(EventState::Consumed);
			} else if key_match(key, self.key_config.keys.home) {
				self.move_selection(ScrollType::Home);
				return Ok(EventState::Consumed);
			} else if key_match(key, self.key_config.keys.end) {
				self.move_selection(ScrollType::End);
				return Ok(EventState::Consumed);
			} else if key_match(key, self.key_config.keys.enter) {
				self.open_selected();
				return Ok(EventState::Consumed);
			}
		}

		Ok(EventState::NotConsumed)
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;
		self.update()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[test]
	fn opening_a_linked_worktree_queues_its_repository_path(
	) -> Result<()> {
		let env = Environment::test_env();
		let queue = env.queue.clone();
		let mut tab = Worktrees::new(&env);
		let linked_path = PathBuf::from("/tmp/gitui-linked-worktree");
		tab.entries.push(WorktreeInfo {
			path: linked_path.clone(),
			branch: Some("feature".to_string()),
			head: None,
			is_current: false,
			is_locked: false,
			is_valid: true,
		});
		tab.table_state.get_mut().select(Some(0));

		tab.open_selected();

		let Some(InternalEvent::OpenRepoPath {
			repo: RepoPath::Path(path),
		}) = queue.pop()
		else {
			anyhow::bail!("expected selected worktree to be queued");
		};
		assert_eq!(path, linked_path);

		Ok(())
	}
}
