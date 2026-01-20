use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::config::{
    APP_ID, CSS_PATH, ICON_LIBRARY, ICON_SEARCH, ICON_SETTINGS,
    WINDOW_DEFAULT_HEIGHT, WINDOW_DEFAULT_WIDTH,
};
use crate::playback::PlaybackController;
use crate::storage::Database;
use crate::ui::{
    build_header, build_library_view, build_mini_player, build_now_playing_view,
    build_search_view, build_settings_view,
};

/// Build and run the application
pub fn run() {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(on_activate);
    app.run();
}

fn on_activate(app: &adw::Application) {
    load_css();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Musika")
        .default_width(WINDOW_DEFAULT_WIDTH)
        .default_height(WINDOW_DEFAULT_HEIGHT)
        .build();

    // Initialize database
    let database = match Database::new() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e);
            return;
        }
    };

    let main_stack = adw::ViewStack::new();
    let playback_controller = PlaybackController::new();

    // Set database on playback controller for recent plays tracking
    playback_controller.set_database(database.clone());

    // Build views
    let search_view = build_search_view(playback_controller.clone(), database.clone());
    let library_view = build_library_view(playback_controller.clone(), database.clone());
    let settings_view = build_settings_view();
    let now_playing_view = build_now_playing_view(playback_controller.clone());

    // Add views to stack with icons
    let search_page = main_stack.add_titled(&search_view, Some("search"), "Search");
    search_page.set_icon_name(Some(ICON_SEARCH));

    let library_page = main_stack.add_titled(&library_view, Some("library"), "Library");
    library_page.set_icon_name(Some(ICON_LIBRARY));

    // Now playing is not in the switcher - only accessible from mini player
    main_stack.add_named(&now_playing_view, Some("now_playing"));

    let settings_page = main_stack.add_titled(&settings_view, Some("settings"), "Settings");
    settings_page.set_icon_name(Some(ICON_SETTINGS));

    // Build header with view switcher
    let header = build_header(&main_stack);

    // Build bottom switcher bar (for narrow widths)
    let switcher_bar = adw::ViewSwitcherBar::new();
    switcher_bar.set_stack(Some(&main_stack));
    switcher_bar.set_reveal(true);

    // Build mini player
    let mini_player = build_mini_player(playback_controller.clone(), main_stack.clone());

    // Hide mini player initially (will be shown when playback starts)
    mini_player.set_visible(false);

    // Register mini player widget with controller
    playback_controller.set_mini_player_widget(mini_player.clone());

    // Hide mini player when on "now_playing" view, show otherwise (if has played)
    main_stack.connect_visible_child_name_notify(glib::clone!(
        #[strong]
        playback_controller,
        move |stack| {
            let is_now_playing = stack
                .visible_child_name()
                .map(|name| name == "now_playing")
                .unwrap_or(false);

            // Track whether now playing view is visible
            playback_controller.set_now_playing_visible(is_now_playing);

            if is_now_playing {
                playback_controller.hide_mini_player();
            } else if playback_controller.has_played() {
                playback_controller.show_mini_player();
            }
        }
    ));

    // Assemble layout
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class("app-root");
    root.append(&header);
    root.append(&main_stack);
    root.append(&mini_player);
    root.append(&switcher_bar);

    window.set_content(Some(&root));
    window.present();
}

fn load_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_path(CSS_PATH);

    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
