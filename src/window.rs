/* SPDX-License-Identifier: GPL-3.0-or-later */

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::batch_view::BatchView;
use crate::comic::{discover_comics, ComicFile};
use crate::single_view::SingleView;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/dmfigueroa/ComicName/window.ui")]
    pub struct ComicNameWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub content_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub open_comic_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub open_folder_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub single_host: TemplateChild<gtk::Box>,
        #[template_child]
        pub batch_host: TemplateChild<gtk::Box>,
        pub single_view: RefCell<Option<Rc<SingleView>>>,
        pub batch_view: RefCell<Option<Rc<BatchView>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ComicNameWindow {
        const NAME: &'static str = "ComicNameWindow";
        type Type = super::ComicNameWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ComicNameWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_callbacks();
        }
    }
    impl WidgetImpl for ComicNameWindow {}
    impl WindowImpl for ComicNameWindow {}
    impl ApplicationWindowImpl for ComicNameWindow {}
    impl AdwApplicationWindowImpl for ComicNameWindow {}
}

glib::wrapper! {
    pub struct ComicNameWindow(ObjectSubclass<imp::ComicNameWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl ComicNameWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    fn setup_callbacks(&self) {
        let weak = self.downgrade();
        self.imp().open_comic_button.connect_clicked(move |_| {
            if let Some(window) = weak.upgrade() {
                window.choose_comic();
            }
        });

        let weak = self.downgrade();
        self.imp().open_folder_button.connect_clicked(move |_| {
            if let Some(window) = weak.upgrade() {
                window.choose_folder();
            }
        });
    }

    pub(crate) fn choose_comic(&self) {
        let dialog = gtk::FileDialog::builder().title("Open Comic").build();
        let filter = comic_filter();
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));

        let weak = self.downgrade();
        dialog.open(Some(self), None::<&gio::Cancellable>, move |result| {
            let Some(window) = weak.upgrade() else { return };
            match result {
                Ok(file) => window.open_path(file.path()),
                Err(error) if error.matches(gtk::DialogError::Dismissed) => {}
                Err(error) => window.show_error(&format!("{error:#}")),
            }
        });
    }

    pub(crate) fn choose_folder(&self) {
        let dialog = gtk::FileDialog::builder()
            .title("Open Comic Folder")
            .build();
        let weak = self.downgrade();
        dialog.select_folder(Some(self), None::<&gio::Cancellable>, move |result| {
            let Some(window) = weak.upgrade() else { return };
            match result {
                Ok(folder) => window.open_path(folder.path()),
                Err(error) if error.matches(gtk::DialogError::Dismissed) => {}
                Err(error) => window.show_error(&format!("{error:#}")),
            }
        });
    }

    pub(crate) fn open_path(&self, path: Option<PathBuf>) {
        let Some(path) = path else {
            self.show_error("The selected location is not available as a local document");
            return;
        };
        match discover_comics(&path) {
            Ok(files) if files.is_empty() => self.show_error("No supported comic files were found"),
            Ok(files) if path.is_dir() => self.show_batch(files, path),
            Ok(mut files) => self.show_single(files.remove(0)),
            Err(error) => self.show_error(&format!("{error:#}")),
        }
    }

    fn show_single(&self, file: ComicFile) {
        clear_box(&self.imp().single_host);
        self.imp().batch_view.borrow_mut().take();
        let view = SingleView::new(file, self);
        self.imp().single_host.append(&view.root());
        self.imp().single_view.replace(Some(view));
        self.imp().content_stack.set_visible_child_name("single");
    }

    fn show_batch(&self, files: Vec<ComicFile>, root: PathBuf) {
        clear_box(&self.imp().batch_host);
        self.imp().single_view.borrow_mut().take();
        let view = BatchView::new(files, root, self);
        self.imp().batch_host.append(&view.root());
        self.imp().batch_view.replace(Some(view));
        self.imp().content_stack.set_visible_child_name("batch");
    }

    pub(crate) fn show_welcome(&self) {
        self.imp().content_stack.set_visible_child_name("welcome");
    }

    pub(crate) fn api_key(&self) -> String {
        gio::Settings::new("com.dmfigueroa.ComicName")
            .string("comic-vine-api-key")
            .into()
    }

    pub(crate) fn show_success(&self, message: &str) {
        self.imp().toast_overlay.add_toast(adw::Toast::new(message));
    }

    pub(crate) fn show_error(&self, message: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading("Something went wrong")
            .body(message)
            .build();
        dialog.add_response("copy", "Copy Details");
        dialog.add_response("close", "Close");
        dialog.set_close_response("close");
        dialog.set_default_response(Some("close"));

        let details = message.to_string();
        dialog.connect_response(Some("copy"), move |dialog, _| {
            dialog.clipboard().set_text(&details);
        });
        dialog.present(Some(self));
    }
}

fn comic_filter() -> gtk::FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Comic files"));
    for suffix in ["cb7", "cba", "cbr", "cbt", "cbz", "pdf"] {
        filter.add_suffix(suffix);
    }
    filter
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
