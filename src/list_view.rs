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
    pub view: gtk::ListView,
    pub model: gio::ListStore,
    pub selection: gtk::SelectionModel,
    pub single_selection: Option<gtk::SingleSelection>,
    pub multi_selection: Option<gtk::MultiSelection>,
}

impl DataList {
    pub fn new() -> Self {
        Self::with_selection(SelectionKind::Single)
    }

    pub fn multi() -> Self {
        Self::with_selection(SelectionKind::Multiple)
    }

    pub fn none() -> Self {
        Self::with_selection(SelectionKind::None)
    }

    fn with_selection(selection_kind: SelectionKind) -> Self {
        let model = gio::ListStore::new::<DataRow>();
        let single_selection = match selection_kind {
            SelectionKind::Single => {
                let selection = gtk::SingleSelection::new(Some(model.clone()));
                selection.set_autoselect(false);
                selection.set_can_unselect(true);
                Some(selection)
            }
            _ => None,
        };
        let multi_selection = match selection_kind {
            SelectionKind::Multiple => Some(gtk::MultiSelection::new(Some(model.clone()))),
            _ => None,
        };
        let selection: gtk::SelectionModel = if let Some(single) = single_selection.as_ref() {
            single.clone().upcast()
        } else if let Some(multi) = multi_selection.as_ref() {
            multi.clone().upcast()
        } else {
            gtk::NoSelection::new(Some(model.clone())).upcast()
        };
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

        Self {
            view,
            model,
            selection,
            single_selection,
            multi_selection,
        }
    }

    pub fn clear(&self) {
        self.model.remove_all();
    }

    pub fn append(&self, title: &str, subtitle: Option<&str>) {
        self.model.append(&DataRow::new(title, subtitle));
    }

    pub fn selected_indices(&self) -> Vec<usize> {
        (0..self.model.n_items())
            .filter(|index| self.selection.is_selected(*index))
            .map(|index| index as usize)
            .collect()
    }

    pub fn select_indices(&self, indices: &[usize]) {
        if let Some(selection) = &self.multi_selection {
            selection.unselect_all();
            for index in indices {
                selection.select_item(*index as u32, true);
            }
        } else if let Some(index) = indices.first() {
            self.single_selection
                .as_ref()
                .expect("single selection is available")
                .set_selected(*index as u32);
        }
    }

    pub fn clear_selection(&self) {
        self.selection.unselect_all();
    }
}

#[derive(Clone, Copy)]
enum SelectionKind {
    Single,
    Multiple,
    None,
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
}
