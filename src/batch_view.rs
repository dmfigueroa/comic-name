use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

use crate::alignment::BatchAlignment;
use crate::comic::{rename_matched, ComicFile};
use crate::comic_vine::{self, Issue, Volume};
use crate::list_view::DataList;
use crate::window::ComicNameWindow;
use crate::workflow_navigation::{WorkflowNavigation, WorkflowPage, WorkflowPageShell};

const ALIGNMENT_ROW_HEIGHT: i32 = 72;

#[derive(Debug)]
pub struct BatchView {
    root: adw::NavigationView,
    navigation: RefCell<WorkflowNavigation>,
    align_page: adw::NavigationPage,
    review_page: adw::NavigationPage,
    search_entry: gtk::SearchEntry,
    series_list: DataList,
    file_list: gtk::ListBox,
    issue_list: gtk::ListBox,
    removed_list: gtk::ListBox,
    preview_list: gtk::ListBox,
    review_button: gtk::Button,
    rename_button: gtk::Button,
    files: Vec<ComicFile>,
    root_directory: PathBuf,
    volumes: RefCell<Vec<Volume>>,
    selected_volume: RefCell<Option<Volume>>,
    alignment: RefCell<Option<BatchAlignment>>,
    generation: Cell<u64>,
}

impl BatchView {
    pub fn new(
        files: Vec<ComicFile>,
        root_directory: PathBuf,
        window: &ComicNameWindow,
    ) -> Rc<Self> {
        let search_entry = gtk::SearchEntry::builder()
            .hexpand(true)
            .placeholder_text("Search for a ComicVine series")
            .build();
        let series_list = DataList::new();
        let file_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["boxed-list"])
            .build();
        let issue_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let removed_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Multiple)
            .css_classes(["boxed-list"])
            .build();
        let preview_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let review_button = review_action_button();
        let rename_button = rename_action_button();

        let choose_content = page_content();
        let search_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        search_row.append(&search_entry);
        choose_content.append(&search_row);
        choose_content.append(&list_scroller(&series_list.widget(), 300));

        let align_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(20)
            .margin_bottom(24)
            .margin_start(20)
            .margin_end(20)
            .build();
        let description = gtk::Label::builder()
            .label(format!(
                "Align {} local comics with the fixed ComicVine issue list.",
                files.len()
            ))
            .xalign(0.0)
            .wrap(true)
            .build();
        description.add_css_class("dim-label");
        align_content.append(&description);

        let shared_adjustment = batch_scroll_adjustment();
        let file_scroller = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .min_content_height(260)
            .min_content_width(330)
            .vadjustment(&shared_adjustment)
            .child(&file_list)
            .build();
        let issue_scroller = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .min_content_height(260)
            .min_content_width(330)
            .vadjustment(&shared_adjustment)
            .child(&issue_list)
            .build();
        let file_column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        file_column.append(&column_label("LOCAL FILES (DRAG TO REORDER)"));
        file_column.append(&file_scroller);
        let issue_column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        issue_column.append(&column_label("COMICVINE ISSUES (FIXED ORDER)"));
        issue_column.append(&issue_scroller);
        let alignment_pane = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .position(500)
            .wide_handle(true)
            .start_child(&file_column)
            .end_child(&issue_column)
            .build();
        align_content.append(&alignment_pane);

        let restore_button = gtk::Button::builder()
            .label("Restore Selected Files")
            .halign(gtk::Align::Start)
            .build();
        let removed_expander = removed_files_expander(&removed_list, &restore_button);
        align_content.append(&removed_expander);
        align_content.append(&review_button);

        let review_content = page_content();
        review_content.append(&list_scroller(&preview_list, 300));
        review_content.append(&rename_button);

        let choose_shell =
            WorkflowPageShell::new("Choose Series", &choose_content, "Open Another Folder");
        let align_shell =
            WorkflowPageShell::new("Align Files", &align_content, "Open Another Folder");
        let review_shell =
            WorkflowPageShell::new("Review Renames", &review_content, "Open Another Folder");
        let root = adw::NavigationView::new();
        root.add(&choose_shell.page());
        root.add(&align_shell.page());
        root.add(&review_shell.page());
        let view = Rc::new(Self {
            root,
            navigation: RefCell::new(WorkflowNavigation::batch()),
            align_page: align_shell.page(),
            review_page: review_shell.page(),
            search_entry,
            series_list,
            file_list,
            issue_list,
            removed_list,
            preview_list,
            review_button,
            rename_button,
            files,
            root_directory,
            volumes: RefCell::new(Vec::new()),
            selected_volume: RefCell::new(None),
            alignment: RefCell::new(None),
            generation: Cell::new(0),
        });
        view.show_unaligned_files();
        view.setup_callbacks(
            window,
            &[
                (choose_shell.action_button(), choose_shell.back_button()),
                (align_shell.action_button(), align_shell.back_button()),
                (review_shell.action_button(), review_shell.back_button()),
            ],
            &restore_button,
        );
        view
    }

    pub fn root(&self) -> adw::NavigationView {
        self.root.clone()
    }

    fn setup_callbacks(
        self: &Rc<Self>,
        window: &ComicNameWindow,
        header_buttons: &[(gtk::Button, gtk::Button)],
        restore_button: &gtk::Button,
    ) {
        for (button, _) in header_buttons {
            let weak_window = window.downgrade();
            button.connect_clicked(move |_| {
                if let Some(window) = weak_window.upgrade() {
                    window.choose_folder();
                }
            });
        }
        for (_, button) in header_buttons {
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
                if view.navigation.borrow_mut().back() == Some(WorkflowPage::ChooseSeries) {
                    view.series_list.clear_selection();
                }
            }
        });

        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        self.search_entry.connect_search_changed(move |_| {
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
        restore_button.connect_clicked(move |_| {
            let Some(view) = weak_self.upgrade() else {
                return;
            };
            let indices = selected_indices(&view.removed_list);
            if indices.is_empty() {
                return;
            }
            let restored = view
                .alignment
                .borrow_mut()
                .as_mut()
                .map(|alignment| alignment.restore(&indices))
                .unwrap_or_default();
            view.refresh_alignment(Some(restored));
        });

        let weak_self = Rc::downgrade(self);
        self.review_button.connect_clicked(move |_| {
            if let Some(view) = weak_self.upgrade() {
                if view.navigation.borrow_mut().advance() == Some(WorkflowPage::ReviewRenames) {
                    view.root.push(&view.review_page);
                }
            }
        });

        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        self.rename_button.connect_clicked(move |_| {
            if let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade()) {
                view.confirm_rename(&window);
            }
        });
    }

    fn show_unaligned_files(self: &Rc<Self>) {
        clear_list(&self.file_list);
        clear_list(&self.issue_list);
        clear_list(&self.removed_list);
        for (index, file) in self.files.iter().enumerate() {
            self.file_list
                .append(&self.file_row(index, &self.display_file(file)));
            self.issue_list.append(&data_row("", None));
        }
        clear_list(&self.preview_list);
        self.preview_list
            .append(&status_label("Choose a series to load its issues."));
    }

    fn search(self: &Rc<Self>, window: &ComicNameWindow) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.volumes.borrow_mut().clear();
        self.selected_volume.replace(None);
        self.alignment.replace(None);
        self.review_button.set_sensitive(false);
        self.rename_button.set_sensitive(false);
        self.series_list.set_placeholder("Searching ComicVine...");
        self.show_unaligned_files();

        let api_key = window.api_key();
        let query = self.search_entry.text().to_string();
        if query.trim().is_empty() {
            self.series_list
                .set_placeholder("Type a series name to search");
            return;
        }
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
        match result {
            Ok(volumes) if volumes.is_empty() => {
                self.series_list.set_placeholder("No series found");
            }
            Ok(volumes) => {
                self.series_list.clear();
                for volume in &volumes {
                    self.series_list
                        .append(&volume.name, Some(&volume_subtitle(volume)));
                }
                self.volumes.replace(volumes);
            }
            Err(error) => {
                self.series_list.set_placeholder("Search failed");
                window.show_error(&format!("{error:#}"));
            }
        }
    }

    fn select_volume(self: &Rc<Self>, index: usize, window: &ComicNameWindow) {
        let Some(volume) = self.volumes.borrow().get(index).cloned() else {
            return;
        };
        self.selected_volume.replace(Some(volume.clone()));
        self.alignment.replace(None);
        self.rename_button.set_sensitive(false);
        self.show_unaligned_files();
        clear_list(&self.issue_list);
        self.issue_list.append(&data_row("Loading issues...", None));
        let choosing_series = self.navigation.borrow().current() == WorkflowPage::ChooseSeries;
        if choosing_series
            && self.navigation.borrow_mut().advance() == Some(WorkflowPage::AlignFiles)
        {
            self.root.push(&self.align_page);
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

    fn finish_issues(
        self: &Rc<Self>,
        result: anyhow::Result<Vec<Issue>>,
        window: &ComicNameWindow,
    ) {
        match result {
            Ok(issues) if issues.is_empty() => {
                clear_list(&self.issue_list);
                self.issue_list.append(&data_row("No issues found", None));
            }
            Ok(issues) => {
                self.alignment
                    .replace(Some(BatchAlignment::new(self.files.clone(), issues)));
                self.refresh_alignment(None);
            }
            Err(error) => {
                clear_list(&self.issue_list);
                window.show_error(&format!("{error:#}"));
            }
        }
    }

    fn refresh_alignment(self: &Rc<Self>, selection: Option<Vec<usize>>) {
        clear_list(&self.file_list);
        clear_list(&self.issue_list);
        clear_list(&self.removed_list);
        clear_list(&self.preview_list);
        let alignment = self.alignment.borrow();
        let Some(alignment) = alignment.as_ref() else {
            return;
        };

        for index in 0..alignment.row_count() {
            let file_name = alignment
                .file(index)
                .map(|file| self.display_file(file))
                .unwrap_or_default();
            self.file_list.append(&self.file_row(index, &file_name));
            let issue = alignment.issue(index);
            let issue_title = issue
                .map(|issue| {
                    format!(
                        "#{} - {}",
                        issue.issue_number,
                        issue.name.as_deref().unwrap_or("Untitled issue")
                    )
                })
                .unwrap_or_default();
            if let Some(issue) = issue {
                let issue_date = issue.release_date().unwrap_or("Unknown release date");
                self.issue_list
                    .append(&data_row(&issue_title, Some(issue_date)));
            } else {
                self.issue_list.append(&data_row("", None));
            }
        }
        for index in 0..alignment.removed_count() {
            if let Some(file) = alignment.removed_file(index) {
                self.removed_list
                    .append(&data_row(&self.display_file(file), None));
            }
        }

        let matches = self
            .selected_volume
            .borrow()
            .as_ref()
            .map(|volume| alignment.matched_files(volume))
            .unwrap_or_default();
        for file in &matches {
            match file.target_path() {
                Ok(target) => {
                    self.preview_list.append(&data_row(
                        &self.display_file(file),
                        Some(&display_path(&self.root_directory, &target)),
                    ));
                }
                Err(error) => self.preview_list.append(&data_row(
                    &self.display_file(file),
                    Some(&format!("Cannot rename: {error}")),
                )),
            }
        }
        if matches.is_empty() {
            self.preview_list.append(&status_label(
                "No rows currently contain both a file and an issue.",
            ));
        }
        self.rename_button.set_sensitive(!matches.is_empty());
        self.review_button.set_sensitive(!matches.is_empty());
        self.rename_button
            .set_label(&format!("Rename {} Comics", matches.len()));

        if let Some(index) = selection.and_then(|indices| indices.first().copied()) {
            if let Some(row) = self.file_list.row_at_index(index as i32) {
                self.file_list.select_row(Some(&row));
            }
        }
    }

    fn confirm_rename(self: &Rc<Self>, window: &ComicNameWindow) {
        let Some(volume) = self.selected_volume.borrow().clone() else {
            return;
        };
        let alignment = self.alignment.borrow();
        let Some(alignment) = alignment.as_ref() else {
            return;
        };
        let files = alignment.matched_files(&volume);
        let targets = files
            .iter()
            .map(ComicFile::target_path)
            .collect::<anyhow::Result<Vec<_>>>();
        if let Err(error) = targets {
            window.show_error(&format!("{error:#}"));
            return;
        }
        let details = format!(
            "The {} filenames shown in the preview will be applied in one batch.",
            files.len()
        );
        let dialog = adw::AlertDialog::builder()
            .heading(format!("Rename {} comics?", files.len()))
            .body(details)
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("rename", "Rename All");
        dialog.set_close_response("cancel");
        dialog.set_default_response(Some("rename"));
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        let weak_self = Rc::downgrade(self);
        let weak_window = window.downgrade();
        dialog.connect_response(Some("rename"), move |_, _| {
            let (Some(view), Some(window)) = (weak_self.upgrade(), weak_window.upgrade()) else {
                return;
            };
            let Some(volume) = view.selected_volume.borrow().clone() else {
                return;
            };
            let mut files = view
                .alignment
                .borrow()
                .as_ref()
                .map(|alignment| alignment.matched_files(&volume))
                .unwrap_or_default();
            match rename_matched(&mut files) {
                Ok(count) => {
                    window.show_welcome();
                    window.show_success(&format!("Renamed {count} comics"));
                }
                Err(error) => window.show_error(&format!("{error:#}")),
            }
        });
        dialog.present(Some(window));
    }

    fn display_file(&self, file: &ComicFile) -> String {
        display_path(&self.root_directory, &file.path)
    }

    fn file_row(self: &Rc<Self>, index: usize, title: &str) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        row.set_height_request(ALIGNMENT_ROW_HEIGHT);
        let handle = gtk::Button::builder()
            .icon_name("list-drag-handle-symbolic")
            .tooltip_text("Drag to reorder")
            .css_classes(["flat"])
            .build();
        let menu_button = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .tooltip_text("Row actions")
            .css_classes(["flat"])
            .build();
        let action_row = adw::ActionRow::builder()
            .title(title)
            .title_lines(1)
            .build();
        action_row.add_prefix(&handle);
        action_row.add_suffix(&menu_button);
        row.set_child(Some(&action_row));

        let menu = gtk::Popover::new();
        let menu_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        for (label, action) in [
            ("Move Up", RowAction::MoveUp),
            ("Move Down", RowAction::MoveDown),
            ("Remove", RowAction::Remove),
        ] {
            let button = gtk::Button::with_label(label);
            button.set_hexpand(true);
            button.set_halign(gtk::Align::Fill);
            let weak_self = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(view) = weak_self.upgrade() {
                    view.apply_row_action(index, action);
                }
            });
            menu_content.append(&button);
        }
        menu.set_child(Some(&menu_content));
        menu_button.set_popover(Some(&menu));

        let source = gtk::DragSource::builder()
            .actions(gtk::gdk::DragAction::MOVE)
            .build();
        source.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(
                &(index as u32).to_value(),
            ))
        });
        handle.add_controller(source);

        let target = gtk::DropTarget::new(u32::static_type(), gtk::gdk::DragAction::MOVE);
        let weak_self = Rc::downgrade(self);
        target.connect_drop(move |_, value, _, _| {
            let Some(view) = weak_self.upgrade() else {
                return false;
            };
            let Ok(from) = value.get::<u32>() else {
                return false;
            };
            view.move_file(from as usize, index)
        });
        row.add_controller(target);
        row
    }

    fn apply_row_action(self: &Rc<Self>, index: usize, action: RowAction) {
        match action {
            RowAction::MoveUp => {
                if let Some(selection) = self
                    .alignment
                    .borrow_mut()
                    .as_mut()
                    .and_then(|alignment| alignment.move_up(index, index))
                {
                    self.refresh_alignment(Some(selection.collect()));
                }
            }
            RowAction::MoveDown => {
                if let Some(selection) = self
                    .alignment
                    .borrow_mut()
                    .as_mut()
                    .and_then(|alignment| alignment.move_down(index, index))
                {
                    self.refresh_alignment(Some(selection.collect()));
                }
            }
            RowAction::Remove => {
                if self
                    .alignment
                    .borrow_mut()
                    .as_mut()
                    .is_some_and(|alignment| alignment.remove(index, index))
                {
                    self.refresh_alignment(None);
                }
            }
        }
    }

    fn move_file(self: &Rc<Self>, from: usize, to: usize) -> bool {
        let selection = self
            .alignment
            .borrow_mut()
            .as_mut()
            .and_then(|alignment| alignment.move_file(from, to));
        if let Some(selection) = selection {
            self.refresh_alignment(Some(selection.collect()));
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy)]
enum RowAction {
    MoveUp,
    MoveDown,
    Remove,
}

fn selected_indices(list: &gtk::ListBox) -> Vec<usize> {
    let mut indices = list
        .selected_rows()
        .iter()
        .map(|row| row.index() as usize)
        .collect::<Vec<_>>();
    indices.sort_unstable();
    indices
}

fn batch_scroll_adjustment() -> gtk::Adjustment {
    gtk::Adjustment::new(0.0, 0.0, 0.0, 1.0, 10.0, 0.0)
}

fn removed_files_expander(
    removed_list: &gtk::ListBox,
    restore_button: &gtk::Button,
) -> adw::ExpanderRow {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    content.append(&list_scroller(removed_list, 90));
    content.append(restore_button);

    let row = adw::ExpanderRow::builder().title("Removed files").build();
    row.add_row(&content);
    row
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

fn review_action_button() -> gtk::Button {
    let button = gtk::Button::builder()
        .label("Review Renames")
        .sensitive(false)
        .halign(gtk::Align::End)
        .build();
    button.add_css_class("pill");
    button
}

fn rename_action_button() -> gtk::Button {
    let button = gtk::Button::builder()
        .label("Rename Comics")
        .sensitive(false)
        .halign(gtk::Align::End)
        .build();
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button
}

fn column_label(text: &str) -> gtk::Label {
    let label = gtk::Label::builder().label(text).xalign(0.0).build();
    label.add_css_class("caption");
    label
}

fn list_scroller<W: IsA<gtk::Widget>>(list: &W, minimum_height: i32) -> gtk::ScrolledWindow {
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

fn data_row(title: &str, subtitle: Option<&str>) -> adw::ActionRow {
    let title = glib::markup_escape_text(title);
    let row = adw::ActionRow::builder()
        .title(title)
        .title_lines(1)
        .subtitle_lines(1)
        .height_request(ALIGNMENT_ROW_HEIGHT)
        .build();
    if let Some(subtitle) = subtitle.filter(|subtitle| !subtitle.is_empty()) {
        row.set_subtitle(&glib::markup_escape_text(subtitle));
    }
    row
}

fn status_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .margin_top(12)
        .margin_bottom(12)
        .build()
}

fn clear_list(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_widgets_start_with_valid_public_properties() {
        if !gtk::is_initialized() {
            gtk::init().expect("GTK must initialize for this test");
        }
        let adjustment = batch_scroll_adjustment();
        let choices = DataList::new();
        let content = gtk::Label::new(Some("Series results"));
        let shell = WorkflowPageShell::new("Choose Series", &content, "Open Another Folder");
        let removed_list = gtk::ListBox::new();
        let restore_button = gtk::Button::with_label("Restore Selected Files");
        let removed_files: adw::ExpanderRow =
            removed_files_expander(&removed_list, &restore_button);

        assert!(adjustment.lower() + adjustment.page_size() <= adjustment.upper());
        let selection = choices
            .single_selection
            .expect("choices use single selection");
        assert!(!selection.is_autoselect());
        assert!(selection.can_unselect());
        assert_eq!(shell.page().title(), "Choose Series");
        assert_eq!(
            shell.action_button().label().as_deref(),
            Some("Open Another Folder")
        );
        assert_eq!(shell.clamp().maximum_size(), 960);
        assert_eq!(removed_files.title(), "Removed files");
        assert!(removed_list.is_ancestor(&removed_files));
        assert!(restore_button.is_ancestor(&removed_files));
    }

    #[test]
    fn alignment_data_rows_have_a_fixed_height_for_row_correlation() {
        if !gtk::is_initialized() {
            gtk::init().expect("GTK must initialize for this test");
        }

        let row = data_row(
            "#3 - Too Many Wars Spoil the Battleship-fund Broth",
            Some("2024-08-01"),
        );

        assert_eq!(row.height_request(), ALIGNMENT_ROW_HEIGHT);
        assert_eq!(row.title_lines(), 1);
    }

    #[test]
    fn batch_actions_have_only_the_rename_primary_action() {
        if !gtk::is_initialized() {
            gtk::init().expect("GTK must initialize for this test");
        }

        let actions = [review_action_button(), rename_action_button()];
        let suggested_actions = actions
            .iter()
            .filter(|button| button.has_css_class("suggested-action"))
            .count();

        assert_eq!(suggested_actions, 1);
        assert!(!actions[0].has_css_class("suggested-action"));
        assert!(actions[1].has_css_class("suggested-action"));
    }
}
