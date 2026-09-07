use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

use crate::comic::{rename_matched, ComicFile, ComicMatch};
use crate::comic_vine::{self, Issue, Volume};
use crate::list_view::DataList;
use crate::window::ComicNameWindow;
use crate::workflow_navigation::{WorkflowNavigation, WorkflowPage, WorkflowPageShell};

#[derive(Debug)]
pub struct SingleView {
    root: adw::NavigationView,
    navigation: RefCell<WorkflowNavigation>,
    issue_page: adw::NavigationPage,
    search_entry: gtk::SearchEntry,
    search_button: gtk::Button,
    series_list: DataList,
    issue_list: DataList,
    file: RefCell<ComicFile>,
    volumes: RefCell<Vec<Volume>>,
    issues: RefCell<Vec<Issue>>,
    selected_volume: RefCell<Option<Volume>>,
    generation: Cell<u64>,
}

impl SingleView {
    pub fn new(file: ComicFile, window: &ComicNameWindow) -> Rc<Self> {
        let search_entry = gtk::SearchEntry::builder()
            .hexpand(true)
            .placeholder_text("Search for a ComicVine series")
            .build();
        let search_button = gtk::Button::builder().label("Search").build();
        search_button.add_css_class("suggested-action");
        let series_list = DataList::new();
        let issue_list = DataList::new();

        let choose_content = page_content();
        let file_label = gtk::Label::builder()
            .label(file.display_name())
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        file_label.add_css_class("dim-label");
        choose_content.append(&file_label);

        let search_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        search_row.append(&search_entry);
        search_row.append(&search_button);
        choose_content.append(&search_row);
        choose_content.append(&list_scroller(&series_list.view, 300));

        let issue_content = page_content();
        issue_content.append(&list_scroller(&issue_list.view, 360));

        let choose_shell =
            WorkflowPageShell::new("Choose Series", &choose_content, "Open Another Comic");
        let issue_shell =
            WorkflowPageShell::new("Choose Issue", &issue_content, "Open Another Comic");
        let root = adw::NavigationView::new();
        root.add(&choose_shell.page());
        root.add(&issue_shell.page());
        let view = Rc::new(Self {
            root,
            navigation: RefCell::new(WorkflowNavigation::single()),
            issue_page: issue_shell.page(),
            search_entry,
            search_button,
            series_list,
            issue_list,
            file: RefCell::new(file),
            volumes: RefCell::new(Vec::new()),
            issues: RefCell::new(Vec::new()),
            selected_volume: RefCell::new(None),
            generation: Cell::new(0),
        });
        view.setup_callbacks(
            window,
            &[choose_shell.action_button(), issue_shell.action_button()],
            &[choose_shell.back_button(), issue_shell.back_button()],
        );
        view
    }

    pub fn root(&self) -> adw::NavigationView {
        self.root.clone()
    }

    fn setup_callbacks(
        self: &Rc<Self>,
        window: &ComicNameWindow,
        new_buttons: &[gtk::Button],
        back_buttons: &[gtk::Button],
    ) {
        for button in new_buttons {
            let weak_window = window.downgrade();
            button.connect_clicked(move |_| {
                if let Some(window) = weak_window.upgrade() {
                    window.choose_comic();
                }
            });
        }
        for button in back_buttons {
            let weak_self = Rc::downgrade(self);
            let weak_window = window.downgrade();
            button.connect_clicked(move |_| {
                let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade())
                else {
                    return;
                };
                if view.navigation.borrow().current() == WorkflowPage::ChooseSeries {
                    window.show_welcome();
                } else {
                    view.root.pop();
                }
            });
        }
        let weak_self = Rc::downgrade(self);
        self.root.connect_popped(move |_, _| {
            if let Some(view) = weak_self.upgrade() {
                match view.navigation.borrow_mut().back() {
                    Some(WorkflowPage::ChooseSeries) => view.series_list.clear_selection(),
                    Some(WorkflowPage::ChooseIssue) => view.issue_list.clear_selection(),
                    _ => {}
                }
            }
        });

        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        self.search_button.connect_clicked(move |_| {
            if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade()) {
                view.search(&window);
            }
        });
        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        self.search_entry.connect_activate(move |_| {
            if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade()) {
                view.search(&window);
            }
        });

        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        self.series_list
            .single_selection
            .as_ref()
            .expect("series list uses single selection")
            .connect_selected_notify(move |selection| {
                if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade()) {
                    let index = selection.selected();
                    if index != gtk::INVALID_LIST_POSITION {
                        view.select_volume(index as usize, &window);
                    }
                }
            });

        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        self.issue_list
            .single_selection
            .as_ref()
            .expect("issue list uses single selection")
            .connect_selected_notify(move |selection| {
                if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade()) {
                    let index = selection.selected();
                    if index != gtk::INVALID_LIST_POSITION {
                        view.select_issue(index as usize, &window);
                    }
                }
            });
    }

    fn search(self: &Rc<Self>, window: &ComicNameWindow) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.search_button.set_sensitive(false);
        self.volumes.borrow_mut().clear();
        self.issues.borrow_mut().clear();
        self.selected_volume.replace(None);
        self.clear_assignment();
        self.series_list.clear();
        self.issue_list.clear();
        self.series_list.append("Searching ComicVine...", None);

        let api_key = window.api_key();
        let query = self.search_entry.text().to_string();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(comic_vine::search_volumes(&api_key, &query));
        });
        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            match receiver.try_recv() {
                Ok(result) => {
                    if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade())
                    {
                        if view.generation.get() == generation {
                            view.finish_search(result, &window);
                        }
                    }
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade())
                    {
                        if view.generation.get() == generation {
                            view.finish_search(
                                Err(anyhow::anyhow!(
                                    "ComicVine search stopped before returning a result"
                                )),
                                &window,
                            );
                        }
                    }
                    glib::ControlFlow::Break
                }
            }
        });
    }

    fn finish_search(&self, result: anyhow::Result<Vec<Volume>>, window: &ComicNameWindow) {
        self.search_button.set_sensitive(true);
        self.series_list.clear();
        match result {
            Ok(volumes) if volumes.is_empty() => {
                self.series_list.append("No series found", None);
            }
            Ok(volumes) => {
                for volume in &volumes {
                    self.series_list
                        .append(&volume.name, Some(&volume_subtitle(volume)));
                }
                self.volumes.replace(volumes);
            }
            Err(error) => window.show_error(&format!("{error:#}")),
        }
    }

    fn select_volume(self: &Rc<Self>, index: usize, window: &ComicNameWindow) {
        let Some(volume) = self.volumes.borrow().get(index).cloned() else {
            return;
        };
        self.selected_volume.replace(Some(volume.clone()));
        self.issues.borrow_mut().clear();
        self.clear_assignment();
        self.issue_list.clear();
        self.issue_list.append("Loading issues...", None);
        let choosing_series = self.navigation.borrow().current() == WorkflowPage::ChooseSeries;
        if choosing_series
            && self.navigation.borrow_mut().advance() == Some(WorkflowPage::ChooseIssue)
        {
            self.root.push(&self.issue_page);
        }
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);

        let api_key = window.api_key();
        let volume_id = volume.id;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(comic_vine::issues_for_volume(&api_key, volume_id));
        });
        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            match receiver.try_recv() {
                Ok(result) => {
                    if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade())
                    {
                        let current_id =
                            view.selected_volume.borrow().as_ref().map(|value| value.id);
                        if view.generation.get() == generation && current_id == Some(volume_id) {
                            view.finish_issues(result, &window);
                        }
                    }
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade())
                    {
                        let current_id =
                            view.selected_volume.borrow().as_ref().map(|value| value.id);
                        if view.generation.get() == generation && current_id == Some(volume_id) {
                            view.finish_issues(
                                Err(anyhow::anyhow!(
                                    "ComicVine issue loading stopped before returning a result"
                                )),
                                &window,
                            );
                        }
                    }
                    glib::ControlFlow::Break
                }
            }
        });
    }

    fn finish_issues(&self, result: anyhow::Result<Vec<Issue>>, window: &ComicNameWindow) {
        self.issue_list.clear();
        match result {
            Ok(issues) if issues.is_empty() => {
                self.issue_list.append("No issues found", None);
            }
            Ok(issues) => {
                for issue in &issues {
                    self.issue_list
                        .append(&issue_title(issue), Some(&issue_subtitle(issue)));
                }
                self.issues.replace(issues);
            }
            Err(error) => window.show_error(&format!("{error:#}")),
        }
    }

    fn select_issue(self: &Rc<Self>, index: usize, window: &ComicNameWindow) {
        let Some(volume) = self.selected_volume.borrow().clone() else {
            return;
        };
        let Some(issue) = self.issues.borrow().get(index).cloned() else {
            return;
        };
        let mut file = self.file.borrow_mut();
        file.comic_match = Some(ComicMatch { volume, issue });
        match file.target_path() {
            Ok(_) => {
                drop(file);
                self.issue_list.clear_selection();
                self.review_rename(window);
            }
            Err(error) => {
                window.show_error(&format!("{error:#}"));
            }
        }
    }

    fn clear_assignment(&self) {
        self.file.borrow_mut().comic_match = None;
    }

    fn review_rename(self: &Rc<Self>, window: &ComicNameWindow) {
        let file = self.file.borrow();
        let Ok(target) = file.target_path() else {
            return;
        };
        let body = format!(
            "{}\n\nwill become\n\n{}",
            file.display_name(),
            target.file_name().unwrap_or_default().to_string_lossy()
        );
        drop(file);

        let dialog = adw::AlertDialog::builder()
            .heading("Rename this comic?")
            .body(body)
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("rename", "Rename");
        dialog.set_close_response("cancel");
        dialog.set_default_response(Some("rename"));
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        dialog.connect_response(Some("rename"), move |_, _| {
            let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade()) else {
                return;
            };
            let result = {
                let mut file = view.file.borrow_mut();
                rename_matched(std::slice::from_mut(&mut file))
            };
            match result {
                Ok(1) => {
                    window.show_welcome();
                    window.show_success("Comic renamed");
                }
                Ok(0) => {
                    window.show_welcome();
                    window.show_success("Comic already has the requested name");
                }
                Ok(_) => unreachable!("single-file workflow renames at most one file"),
                Err(error) => window.show_error(&format!("{error:#}")),
            };
        });
        dialog.present(Some(window));
    }
}

fn page_content() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build()
}

fn list_scroller(list: &gtk::ListView, minimum_height: i32) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .min_content_height(minimum_height)
        .vexpand(true)
        .child(list)
        .build()
}

fn volume_subtitle(volume: &Volume) -> String {
    let year = volume
        .start_year
        .map_or_else(|| "Unknown year".into(), |year| year.to_string());
    let publisher = volume
        .publisher
        .as_ref()
        .map(|publisher| publisher.name.as_str())
        .unwrap_or("Unknown publisher");
    format!("{publisher} ({year})")
}

fn issue_title(issue: &Issue) -> String {
    let title = issue.name.as_deref().unwrap_or("Untitled issue");
    format!("#{} - {title}", issue.issue_number)
}

fn issue_subtitle(issue: &Issue) -> String {
    issue
        .release_date()
        .unwrap_or("Unknown release date")
        .into()
}
