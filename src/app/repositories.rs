//! Repository discovery and board coverage, hosted in the Settings dialog shell.
use super::*;
use gpui::EventEmitter;
use gpui_component::{checkbox::Checkbox, radio::Radio};
use prmarmot_core::board::{RepositoryFilter, RepositoryMode};

pub(super) enum RepositoryEvent {
    Sync,
    Apply(RepositoryFilter),
}

pub(super) struct RepositoriesView {
    pub repos: Vec<String>,
    pub syncing: bool,
    pub status: String,
    filter: RepositoryFilter,
    search: Entity<InputState>,
    scroll: gpui::ScrollHandle,
}

impl EventEmitter<RepositoryEvent> for RepositoriesView {}

impl Render for RepositoriesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let visible: Vec<_> = self
            .repos
            .iter()
            .enumerate()
            .filter(|(_, repo)| repo.to_lowercase().contains(&query))
            .collect();
        v_flex().gap_3()
            .child(h_flex().gap_3()
                .child(Button::new("sync-repositories").label("Sync").disabled(self.syncing)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(RepositoryEvent::Sync))))
                .child(div().text_size(type_size::CAPTION).child(self.status.clone())))
            .child(h_flex().gap_4()
                .children([(RepositoryMode::Include, "Include"), (RepositoryMode::Exclude, "Exclude")]
                    .into_iter().map(|(mode, label)| {
                        Radio::new(label).label(label).checked(self.filter.mode == mode)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.filter.mode = mode;
                                cx.notify();
                            }))
                    })))
            .child(div().text_size(type_size::CAPTION).child(match self.filter.mode {
                RepositoryMode::Include => "Only checked repositories appear on the board. None checked means an empty board.",
                RepositoryMode::Exclude => "Checked repositories are omitted from the board. None checked means all repositories.",
            }))
            .child(Checkbox::new("exclude-archived-repositories")
                .label("Exclude archived repositories")
                .checked(self.filter.exclude_archived)
                .on_click(cx.listener(|this, checked, _, cx| {
                    this.filter.exclude_archived = *checked;
                    cx.notify();
                })))
            .child(Input::new(&self.search).cleanable(true).aria_label("Filter repositories"))
            .child(v_flex().id("repository-list").gap_2().overflow_y_scroll()
                .track_scroll(&self.scroll)
                .max_h((window.viewport_size().height - px(410.)).max(px(100.)).min(px(360.)))
                .when(self.repos.is_empty(), |list| list.child("No repositories synced yet. Click Sync to fetch them."))
                .when(!self.repos.is_empty() && visible.is_empty(), |list| list.child("No repositories match your filter."))
                .children(visible.into_iter().map(|(ix, repo)| {
                    let repo = repo.clone();
                    Checkbox::new(("repository", ix)).label(repo.clone())
                        .checked(self.filter.repos.iter().any(|r| r.eq_ignore_ascii_case(&repo)))
                        .on_click(cx.listener(move |this, checked, _, cx| {
                            this.filter.repos.retain(|r| !r.eq_ignore_ascii_case(&repo));
                            if *checked { this.filter.repos.push(repo.clone()); }
                            cx.notify();
                        }))
                })))
            .child(h_flex().justify_end().gap_2()
                .child(Button::new("cancel-repositories").label("Cancel")
                    .on_click(|_, window, cx| window.close_dialog(cx)))
                .child(Button::new("apply-repositories").primary().label("Apply")
                    .on_click(cx.listener(|this, _, _, cx| cx.emit(RepositoryEvent::Apply(this.filter.clone()))))))
    }
}

impl RootView {
    pub(super) fn show_repositories(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let filter = self.state.read(cx).config.repository_filter.clone();
        let panel = cx.new(|cx: &mut Context<RepositoriesView>| {
            let search =
                cx.new(|cx| InputState::new(window, cx).placeholder("Filter repositories…"));
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.scroll.set_offset(gpui::point(px(0.), px(0.)));
                    cx.notify();
                }
            })
            .detach();
            RepositoriesView {
                repos: self.synced_repos.clone(),
                syncing: self.discovering_repos,
                status: self.repo_status.clone(),
                filter,
                search,
                scroll: gpui::ScrollHandle::new(),
            }
        });
        cx.subscribe_in(
            &panel,
            window,
            |this, panel, event: &RepositoryEvent, window, cx| match event {
                RepositoryEvent::Sync => this.discover_repos(window, cx),
                RepositoryEvent::Apply(filter) => {
                    if let Err(error) = crate::config::save_repository_filter(filter) {
                        panel.update(cx, |panel, cx| {
                            panel.status = error;
                            cx.notify();
                        });
                        return;
                    }
                    this.state.update(cx, |state, cx| {
                        let mut config = state.config.clone();
                        config.repository_filter = filter.clone();
                        state.apply_config(config, cx);
                    });
                    window.close_dialog(cx);
                    this.table.focus_handle(cx).focus(window, cx);
                }
            },
        )
        .detach();
        self.repositories_panel = Some(panel.downgrade());
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Repositories")
                .w(px(560.))
                .child(panel.clone())
        });
    }

    pub(super) fn update_repositories_panel(&self, cx: &mut Context<Self>) {
        if let Some(panel) = self
            .repositories_panel
            .as_ref()
            .and_then(|panel| panel.upgrade())
        {
            panel.update(cx, |panel, cx| {
                panel.repos = self.synced_repos.clone();
                panel.syncing = self.discovering_repos;
                panel.status = self.repo_status.clone();
                cx.notify();
            });
        }
    }
}
