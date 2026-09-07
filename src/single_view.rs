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

#[derive(Debug)]
pub struct SingleView {
    root: gtk::ScrolledWindow,
    search_entry: gtk::SearchEntry,
    search_button: gtk::Button,
    series_list: DataList,
    issue_list: DataList,
    preview_label: gtk::Label,
    rename_button: gtk::Button,
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
        let preview_label = gtk::Label::builder()
            .label("Select a series, then choose one issue.")
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        preview_label.add_css_class("dim-label");
        let rename_button = gtk::Button::builder()
            .label("Review Rename")
            .sensitive(false)
            .halign(gtk::Align::End)
            .build();
        rename_button.add_css_class("suggested-action");
        rename_button.add_css_class("pill");

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        let title_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        let title = gtk::Label::builder()
            .label(format!("Name {}", file.display_name()))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        title.add_css_class("title-2");
        let new_button = gtk::Button::builder().label("Open Another Comic").build();
        title_row.append(&title);
        title_row.append(&new_button);
        content.append(&title_row);

        let search_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        search_row.append(&search_entry);
        search_row.append(&search_button);
        content.append(&search_row);
        content.append(&section_label("1. CHOOSE A COMICVINE SERIES"));
        content.append(&list_scroller(&series_list.view, 170));
        content.append(&section_label("2. CHOOSE ONE ISSUE"));
        content.append(&list_scroller(&issue_list.view, 220));
        content.append(&preview_label);
        content.append(&rename_button);

        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&content)
            .build();
        let view = Rc::new(Self {
            root,
            search_entry,
            search_button,
            series_list,
            issue_list,
            preview_label,
            rename_button,
            file: RefCell::new(file),
            volumes: RefCell::new(Vec::new()),
            issues: RefCell::new(Vec::new()),
            selected_volume: RefCell::new(None),
            generation: Cell::new(0),
        });
        view.setup_callbacks(window, &new_button);
        view
    }

    pub fn root(&self) -> gtk::ScrolledWindow {
        self.root.clone()
    }

    fn setup_callbacks(self: &Rc<Self>, window: &ComicNameWindow, new_button: &gtk::Button) {
        let weak_window = window.downgrade();
        new_button.connect_clicked(move |_| {
            if let Some(window) = weak_window.upgrade() {
                window.choose_comic();
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

        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        self.rename_button.connect_clicked(move |_| {
            if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade()) {
                view.review_rename(&window);
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

    fn select_issue(&self, index: usize, window: &ComicNameWindow) {
        let Some(volume) = self.selected_volume.borrow().clone() else {
            return;
        };
        let Some(issue) = self.issues.borrow().get(index).cloned() else {
            return;
        };
        let mut file = self.file.borrow_mut();
        file.comic_match = Some(ComicMatch { volume, issue });
        match file.target_path() {
            Ok(path) => {
                self.preview_label.set_label(&format!(
                    "New filename: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                self.rename_button.set_sensitive(true);
            }
            Err(error) => {
                self.preview_label
                    .set_label(&format!("Cannot rename: {error:#}"));
                self.rename_button.set_sensitive(false);
                window.show_error(&format!("{error:#}"));
            }
        }
    }

    fn clear_assignment(&self) {
        self.file.borrow_mut().comic_match = None;
        self.preview_label
            .set_label("Select a series, then choose one issue.");
        self.rename_button.set_sensitive(false);
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

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::builder().label(text).xalign(0.0).build();
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label
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
