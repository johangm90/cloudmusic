use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::config::MARGIN_MEDIUM;
use crate::playback::PlaybackController;

const COVER_SIZE: i32 = 320;

/// Builds the now playing view with a clean, minimalist design
pub fn build_now_playing_view(controller: PlaybackController) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    container.set_vexpand(true);
    container.set_hexpand(true);
    container.add_css_class("now-playing-view");

    // Create the background overlay for dynamic coloring
    let background = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    background.set_vexpand(true);
    background.set_hexpand(true);
    background.add_css_class("now-playing-background");

    // LEFT PANEL: Queue (hidden by default)
    let left_panel = gtk4::Revealer::new();
    left_panel.set_transition_type(gtk4::RevealerTransitionType::SlideRight);
    left_panel.set_transition_duration(250);
    left_panel.set_reveal_child(false);

    let queue_panel = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    queue_panel.set_vexpand(true);
    queue_panel.set_width_request(320);
    queue_panel.set_margin_top(MARGIN_MEDIUM);
    queue_panel.set_margin_bottom(MARGIN_MEDIUM);
    queue_panel.set_margin_start(MARGIN_MEDIUM);
    queue_panel.set_margin_end(8);
    queue_panel.add_css_class("side-panel");

    let queue_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    queue_header.set_margin_bottom(8);
    let queue_label = gtk4::Label::new(Some("Up Next"));
    queue_label.add_css_class("panel-title");
    queue_label.set_hexpand(true);
    queue_label.set_xalign(0.0);
    queue_header.append(&queue_label);

    let queue_scroller = gtk4::ScrolledWindow::new();
    queue_scroller.set_vexpand(true);
    queue_scroller.set_hexpand(true);
    queue_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);

    let queue_list = gtk4::ListBox::new();
    queue_list.set_selection_mode(gtk4::SelectionMode::None);
    queue_list.add_css_class("queue-list");
    queue_list.set_activate_on_single_click(true);
    queue_scroller.set_child(Some(&queue_list));

    queue_panel.append(&queue_header);
    queue_panel.append(&queue_scroller);
    left_panel.set_child(Some(&queue_panel));

    // CENTER PANEL: Main content
    let center_panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    center_panel.set_vexpand(true);
    center_panel.set_hexpand(true);
    center_panel.set_valign(gtk4::Align::Center);
    center_panel.set_halign(gtk4::Align::Center);
    center_panel.add_css_class("center-panel");

    // Toggle buttons row at top
    let toggles_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    toggles_row.set_halign(gtk4::Align::Center);
    toggles_row.set_margin_bottom(32);
    toggles_row.add_css_class("toggle-row");

    let queue_toggle = gtk4::ToggleButton::new();
    queue_toggle.set_icon_name("view-list-symbolic");
    queue_toggle.add_css_class("toggle-button");
    queue_toggle.add_css_class("flat");
    queue_toggle.set_tooltip_text(Some("Queue"));

    let lyrics_toggle = gtk4::ToggleButton::new();
    lyrics_toggle.set_icon_name("view-list-bullet-symbolic");
    lyrics_toggle.add_css_class("toggle-button");
    lyrics_toggle.add_css_class("flat");
    lyrics_toggle.set_tooltip_text(Some("Lyrics"));

    toggles_row.append(&queue_toggle);
    toggles_row.append(&lyrics_toggle);

    // Album art - clean rounded rectangle
    let art_container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    art_container.set_halign(gtk4::Align::Center);
    art_container.set_valign(gtk4::Align::Center);
    art_container.add_css_class("art-container");

    let cover_frame = gtk4::Frame::new(None);
    cover_frame.set_halign(gtk4::Align::Center);
    cover_frame.set_valign(gtk4::Align::Center);
    cover_frame.set_size_request(COVER_SIZE, COVER_SIZE);
    cover_frame.add_css_class("cover-container");

    let cover = gtk4::Image::from_icon_name("audio-x-generic-symbolic");
    cover.set_pixel_size(COVER_SIZE);
    cover.set_size_request(COVER_SIZE, COVER_SIZE);
    cover.add_css_class("now-playing-cover-art");
    cover_frame.set_child(Some(&cover));
    art_container.append(&cover_frame);

    // Track info
    let info_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    info_box.set_halign(gtk4::Align::Center);
    info_box.set_margin_top(28);

    let title = gtk4::Label::new(Some("Nothing playing"));
    title.add_css_class("now-playing-title");
    title.set_wrap(true);
    title.set_justify(gtk4::Justification::Center);
    title.set_max_width_chars(35);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.set_lines(1);

    let artist = gtk4::Label::new(Some("Select a song to play"));
    artist.add_css_class("now-playing-artist");
    artist.set_wrap(true);
    artist.set_justify(gtk4::Justification::Center);
    artist.set_max_width_chars(45);

    info_box.append(&title);
    info_box.append(&artist);

    // Playback controls with additional buttons
    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
    controls.set_halign(gtk4::Align::Center);
    controls.set_margin_top(24);
    controls.add_css_class("playback-controls");

    let shuffle = gtk4::ToggleButton::new();
    shuffle.set_icon_name("media-playlist-shuffle-symbolic");
    shuffle.add_css_class("control-button");
    shuffle.add_css_class("control-button-small");
    shuffle.set_tooltip_text(Some("Shuffle"));

    let prev = gtk4::Button::from_icon_name("media-skip-backward-symbolic");
    prev.add_css_class("control-button");
    prev.add_css_class("control-button-secondary");

    let play = gtk4::Button::from_icon_name("media-playback-start-symbolic");
    play.add_css_class("control-button");
    play.add_css_class("play-button");

    let next = gtk4::Button::from_icon_name("media-skip-forward-symbolic");
    next.add_css_class("control-button");
    next.add_css_class("control-button-secondary");

    let repeat = gtk4::ToggleButton::new();
    repeat.set_icon_name("media-playlist-repeat-symbolic");
    repeat.add_css_class("control-button");
    repeat.add_css_class("control-button-small");
    repeat.set_tooltip_text(Some("Repeat"));

    // Wire up playback controls
    play.connect_clicked(glib::clone!(
        #[strong]
        controller,
        move |_| {
            controller.toggle_play_pause();
        }
    ));

    prev.connect_clicked(glib::clone!(
        #[strong]
        controller,
        move |_| {
            controller.play_previous();
        }
    ));

    next.connect_clicked(glib::clone!(
        #[strong]
        controller,
        move |_| {
            controller.play_next();
        }
    ));

    controls.append(&shuffle);
    controls.append(&prev);
    controls.append(&play);
    controls.append(&next);
    controls.append(&repeat);

    // Progress section
    let progress_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    progress_box.set_margin_top(28);
    progress_box.set_width_request(380);
    progress_box.add_css_class("progress-container");

    let progress = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 1.0);
    progress.set_draw_value(false);
    progress.set_hexpand(true);
    progress.set_sensitive(false);
    progress.add_css_class("now-playing-progress");

    // Wire up seeking
    progress.connect_change_value(glib::clone!(
        #[strong]
        controller,
        move |_, _, value| {
            controller.seek(value);
            glib::Propagation::Proceed
        }
    ));

    let time_labels = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let current_time = gtk4::Label::new(Some("0:00"));
    current_time.add_css_class("time-label");
    let total_time = gtk4::Label::new(Some("0:00"));
    total_time.add_css_class("time-label");
    total_time.set_hexpand(true);
    total_time.set_xalign(1.0);

    time_labels.append(&current_time);
    time_labels.append(&total_time);

    progress_box.append(&progress);
    progress_box.append(&time_labels);

    // Assemble center panel
    center_panel.append(&toggles_row);
    center_panel.append(&art_container);
    center_panel.append(&info_box);
    center_panel.append(&controls);
    center_panel.append(&progress_box);

    // RIGHT PANEL: Lyrics (hidden by default)
    let right_panel = gtk4::Revealer::new();
    right_panel.set_transition_type(gtk4::RevealerTransitionType::SlideLeft);
    right_panel.set_transition_duration(250);
    right_panel.set_reveal_child(false);

    let lyrics_panel = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    lyrics_panel.set_vexpand(true);
    lyrics_panel.set_width_request(340);
    lyrics_panel.set_margin_top(MARGIN_MEDIUM);
    lyrics_panel.set_margin_bottom(MARGIN_MEDIUM);
    lyrics_panel.set_margin_start(8);
    lyrics_panel.set_margin_end(MARGIN_MEDIUM);
    lyrics_panel.add_css_class("side-panel");

    let lyrics_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    lyrics_header.set_margin_bottom(8);
    let lyrics_label = gtk4::Label::new(Some("Lyrics"));
    lyrics_label.add_css_class("panel-title");
    lyrics_label.set_hexpand(true);
    lyrics_label.set_xalign(0.0);
    lyrics_header.append(&lyrics_label);

    let lyrics_scroller = gtk4::ScrolledWindow::new();
    lyrics_scroller.set_vexpand(true);
    lyrics_scroller.set_hexpand(true);
    lyrics_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);

    let lyrics_list = gtk4::ListBox::new();
    lyrics_list.set_selection_mode(gtk4::SelectionMode::None);
    lyrics_list.add_css_class("lyrics-list");
    lyrics_scroller.set_child(Some(&lyrics_list));

    lyrics_panel.append(&lyrics_header);
    lyrics_panel.append(&lyrics_scroller);
    right_panel.set_child(Some(&lyrics_panel));

    // Wire up toggle buttons
    queue_toggle.connect_toggled(glib::clone!(
        #[strong]
        left_panel,
        move |btn| {
            left_panel.set_reveal_child(btn.is_active());
        }
    ));

    lyrics_toggle.connect_toggled(glib::clone!(
        #[strong]
        right_panel,
        move |btn| {
            right_panel.set_reveal_child(btn.is_active());
        }
    ));

    // Assemble layout
    background.append(&left_panel);
    background.append(&center_panel);
    background.append(&right_panel);
    container.append(&background);

    // Add keyboard shortcuts
    let key_controller = gtk4::EventControllerKey::new();
    key_controller.connect_key_pressed(glib::clone!(
        #[strong]
        controller,
        #[strong]
        queue_toggle,
        #[strong]
        lyrics_toggle,
        move |_, keyval, _, _| {
            match keyval {
                gtk4::gdk::Key::space => {
                    controller.toggle_play_pause();
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::Left => {
                    controller.play_previous();
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::Right => {
                    controller.play_next();
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::q => {
                    queue_toggle.set_active(!queue_toggle.is_active());
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::l => {
                    lyrics_toggle.set_active(!lyrics_toggle.is_active());
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        }
    ));
    container.add_controller(key_controller);

    // Store references for dynamic background updates
    let bg_ref = Rc::new(RefCell::new(background.clone()));

    // Create dummy drawing areas for visualizer (not visible, but needed for controller)
    let ring1 = gtk4::DrawingArea::new();
    let ring2 = gtk4::DrawingArea::new();
    let ring3 = gtk4::DrawingArea::new();

    // Wire up the controller
    controller.set_ui_elements(
        title.clone(),
        artist.clone(),
        progress.clone(),
        queue_list.clone(),
        play.clone(),
        prev.clone(),
        next.clone(),
        cover.clone(),
        Some(current_time),
        Some(total_time),
    );
    controller.set_lyrics_elements(lyrics_list.clone(), lyrics_scroller.clone());
    controller.set_queue_scroller(queue_scroller.clone());
    controller.set_visualizer_elements(ring1, ring2, ring3, bg_ref);

    container
}
