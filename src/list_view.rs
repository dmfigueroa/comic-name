use std::cell::RefCell;
use std::sync::OnceLock;

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DataRow {
        pub title: RefCell<String>,
        pub subtitle: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DataRow {
        const NAME: &'static str = "ComicNameDataRow";
        type Type = super::DataRow;
    }

    impl ObjectImpl for DataRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
                vec![
                    glib::ParamSpecString::builder("title")
                        .default_value(Some(""))
                        .readwrite()
                        .build(),
                    glib::ParamSpecString::builder("subtitle")
                        .default_value(Some(""))
                        .readwrite()
                        .build(),
                ]
            })
        }

        fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
            match id {
                1 => self.title.replace(value.get().expect("title is a string")),
                2 => self
                    .subtitle
                    .replace(value.get().expect("subtitle is a string")),
                _ => unreachable!(),
            };
        }

        fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
            match id {
                1 => self.title.borrow().to_value(),
                2 => self.subtitle.borrow().to_value(),
                _ => unreachable!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct DataRow(ObjectSubclass<imp::DataRow>);
}

impl DataRow {
    pub fn new(title: &str, subtitle: Option<&str>) -> Self {
        glib::Object::builder()
            .property("title", title)
            .property("subtitle", subtitle.unwrap_or_default())
            .build()
    }

    pub fn title(&self) -> String {
        self.property("title")
    }

    pub fn subtitle(&self) -> String {
        self.property("subtitle")
    }
}

#[derive(Debug)]
pub struct DataList {
    pub model: gio::ListStore,
    pub selection: gtk::SelectionModel,
    pub single_selection: Option<gtk::SingleSelection>,
    stack: gtk::Stack,
    placeholder: gtk::Label,
}

impl DataList {
    pub fn new() -> Self {
        let model = gio::ListStore::new::<DataRow>();
        let single_selection = gtk::SingleSelection::new(Some(model.clone()));
        single_selection.set_autoselect(false);
        single_selection.set_can_unselect(true);
        Self::with_selection(model, single_selection)
    }

    fn with_selection(model: gio::ListStore, single_selection: gtk::SingleSelection) -> Self {
        let selection: gtk::SelectionModel = single_selection.clone().upcast();
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item
                .downcast_ref::<gtk::ListItem>()
                .expect("factory item must be a list item");
            let builder = gtk::Builder::from_resource("/com/dmfigueroa/ComicName/data-row.ui");
            let row = builder
                .object::<adw::ActionRow>("data_row")
                .expect("data row resource is missing");
            item.set_child(Some(&row));
        });
        factory.connect_bind(|_, item| {
            let item = item
                .downcast_ref::<gtk::ListItem>()
                .expect("factory item must be a list item");
            let data = item
                .item()
                .and_downcast::<DataRow>()
                .expect("list item must contain a data row");
            let row = item
                .child()
                .and_downcast::<adw::ActionRow>()
                .expect("list item must contain an action row");
            row.set_title(&data.title());
            row.set_subtitle(&data.subtitle());
        });
        let view = gtk::ListView::builder()
            .model(&selection)
            .factory(&factory)
            .build();
        view.add_css_class("boxed-list");
        let placeholder = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        placeholder.add_css_class("dim-label");
        let stack = gtk::Stack::new();
        stack.add_named(&placeholder, Some("placeholder"));
        stack.add_named(&view, Some("list"));
        stack.set_visible_child_name("placeholder");

        Self {
            model,
            selection,
            single_selection: Some(single_selection),
            stack,
            placeholder,
        }
    }

    pub fn widget(&self) -> gtk::Stack {
        self.stack.clone()
    }

    pub fn clear(&self) {
        self.model.remove_all();
    }

    pub fn append(&self, title: &str, subtitle: Option<&str>) {
        self.model.append(&DataRow::new(title, subtitle));
        self.stack.set_visible_child_name("list");
    }

    pub fn set_placeholder(&self, text: &str) {
        self.model.remove_all();
        self.placeholder.set_text(text);
        self.stack.set_visible_child_name("placeholder");
    }

    #[cfg(test)]
    fn placeholder_text(&self) -> String {
        self.placeholder.text().to_string()
    }

    pub fn clear_selection(&self) {
        self.selection.unselect_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_rows_with_independent_title_and_subtitle_data() {
        let model = gio::ListStore::new::<DataRow>();
        model.append(&DataRow::new("Batman", Some("DC Comics (2014)")));
        assert_eq!(
            model
                .item(0)
                .unwrap()
                .downcast_ref::<DataRow>()
                .unwrap()
                .title(),
            "Batman"
        );
        assert_eq!(
            model.item(0).unwrap().property::<String>("subtitle"),
            "DC Comics (2014)"
        );
    }

    #[test]
    fn placeholder_state_does_not_add_a_selectable_row() {
        let list = DataList::new();

        list.set_placeholder("No series found");

        assert_eq!(list.model.n_items(), 0);
        assert_eq!(list.placeholder_text(), "No series found");
    }
}
