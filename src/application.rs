/* application.rs
 *
 * Copyright 2026 David Figueroa
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::VERSION;
use crate::ComicNameWindow;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct ComicNameApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for ComicNameApplication {
        const NAME: &'static str = "ComicNameApplication";
        type Type = super::ComicNameApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for ComicNameApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<control>q"]);
            obj.set_accels_for_action("app.shortcuts", &["<control>question"]);
        }
    }

    impl ApplicationImpl for ComicNameApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();
            // Get the current window or create one if necessary
            let window = application.active_window().unwrap_or_else(|| {
                let window = ComicNameWindow::new(&*application);
                window.upcast()
            });

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for ComicNameApplication {}
    impl AdwApplicationImpl for ComicNameApplication {}
}

glib::wrapper! {
    pub struct ComicNameApplication(ObjectSubclass<imp::ComicNameApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl ComicNameApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .property("resource-base-path", "/com/dmfigueroa/ComicName")
            .build()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(move |app: &Self, _, _| app.show_preferences())
            .build();
        let shortcuts_action = gio::ActionEntry::builder("shortcuts")
            .activate(move |app: &Self, _, _| app.show_shortcuts())
            .build();
        self.add_action_entries([
            quit_action,
            about_action,
            preferences_action,
            shortcuts_action,
        ]);
    }

    fn show_shortcuts(&self) {
        let builder = gtk::Builder::from_resource("/com/dmfigueroa/ComicName/shortcuts-dialog.ui");
        let dialog = builder
            .object::<adw::Dialog>("shortcuts_dialog")
            .expect("shortcuts dialog resource is missing");
        dialog.present(self.active_window().as_ref());
    }

    fn show_preferences(&self) {
        let dialog = adw::PreferencesDialog::new();
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();
        let api_key = adw::PasswordEntryRow::new();
        api_key.set_title("ComicVine API key");
        api_key.set_show_apply_button(true);
        group.set_title("ComicVine");
        group.set_description(Some("Create an API key at comicvine.gamespot.com/api."));
        group.add(&api_key);
        page.add(&group);
        dialog.add(&page);

        gio::Settings::new("com.dmfigueroa.ComicName")
            .bind("comic-vine-api-key", &api_key, "text")
            .build();
        dialog.present(self.active_window().as_ref());
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("Comic Name")
            .application_icon("com.dmfigueroa.ComicName")
            .developer_name("David Figueroa")
            .version(VERSION)
            .developers(vec!["David Figueroa"])
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(gettext("translator-credits"))
            .copyright("© 2026 David Figueroa")
            .build();

        about.present(Some(&window));
    }
}
