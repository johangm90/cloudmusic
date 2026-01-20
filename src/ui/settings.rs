use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::config::{APP_NAME, APP_VERSION, MARGIN_MEDIUM};

/// Builds the settings view
pub fn build_settings_view() -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_vexpand(true);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
    content.set_margin_top(MARGIN_MEDIUM);
    content.set_margin_bottom(MARGIN_MEDIUM);
    content.set_margin_start(MARGIN_MEDIUM);
    content.set_margin_end(MARGIN_MEDIUM);

    let title = gtk4::Label::new(Some("Settings"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);

    // About section using AdwPreferencesGroup
    let about_group = adw::PreferencesGroup::new();
    about_group.set_title("About");
    about_group.set_description(Some("Application information"));

    let version_row = adw::ActionRow::new();
    version_row.set_title("Version");
    version_row.set_subtitle(APP_VERSION);
    version_row.set_activatable(false);

    let app_row = adw::ActionRow::new();
    app_row.set_title(APP_NAME);
    app_row.set_subtitle("A modern music player built with Rust + GNOME");
    app_row.set_activatable(false);

    about_group.add(&version_row);
    about_group.add(&app_row);

    // Playback section
    let playback_group = adw::PreferencesGroup::new();
    playback_group.set_title("Playback");
    playback_group.set_description(Some("Audio playback settings"));

    let quality_row = adw::ActionRow::new();
    quality_row.set_title("Audio Quality");
    quality_row.set_subtitle("Best available");
    quality_row.set_activatable(false);

    playback_group.add(&quality_row);

    // Interface section
    let interface_group = adw::PreferencesGroup::new();
    interface_group.set_title("Interface");
    interface_group.set_description(Some("Appearance settings"));

    let theme_row = adw::ActionRow::new();
    theme_row.set_title("Theme");
    theme_row.set_subtitle("Follow system preference");
    theme_row.set_activatable(false);

    interface_group.add(&theme_row);

    content.append(&title);
    content.append(&about_group);
    content.append(&playback_group);
    content.append(&interface_group);

    scroller.set_child(Some(&content));
    container.append(&scroller);
    container
}
