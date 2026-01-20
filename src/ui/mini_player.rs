use gtk4::prelude::*;
use libadwaita as adw;

use crate::config::{COVER_SIZE_SMALL, DEFAULT_COVER_PATH, MARGIN_SMALL};
use crate::playback::PlaybackController;

/// Builds the mini player widget shown at the bottom of the window
pub fn build_mini_player(controller: PlaybackController, stack: adw::ViewStack) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    container.add_css_class("mini-player");

    // Mini progress bar
    let progress = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 1.0);
    progress.set_hexpand(true);
    progress.set_draw_value(false);
    progress.add_css_class("mini-progress");

    // Wire up seeking
    progress.connect_change_value(glib::clone!(
        #[strong]
        controller,
        move |_, _, value| {
            controller.seek(value);
            glib::Propagation::Proceed
        }
    ));

    // Controls row
    let controls_row = gtk4::Box::new(gtk4::Orientation::Horizontal, MARGIN_SMALL);
    controls_row.set_margin_top(12);
    controls_row.set_margin_bottom(12);
    controls_row.set_margin_start(20);
    controls_row.set_margin_end(20);

    // Left section: cover and track info
    let left_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    left_box.set_hexpand(true);

    let cover = gtk4::Image::from_file(DEFAULT_COVER_PATH);
    cover.set_pixel_size(COVER_SIZE_SMALL);
    cover.set_overflow(gtk4::Overflow::Hidden);
    cover.add_css_class("album-cover");
    cover.add_css_class("album-cover-small");

    let track_info = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    track_info.set_valign(gtk4::Align::Center);

    let title = gtk4::Label::new(Some("Nothing playing"));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.set_max_width_chars(40);
    title.add_css_class("caption-heading");

    let artist = gtk4::Label::new(Some("Select a song to play"));
    artist.set_xalign(0.0);
    artist.add_css_class("dim-label");
    artist.add_css_class("caption");
    artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    artist.set_max_width_chars(40);

    track_info.append(&title);
    track_info.append(&artist);

    left_box.append(&cover);
    left_box.append(&track_info);

    // Center section: playback controls
    let center_controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    center_controls.set_halign(gtk4::Align::Center);

    let prev = gtk4::Button::from_icon_name("media-skip-backward-symbolic");
    prev.add_css_class("flat");

    let play = gtk4::Button::from_icon_name("media-playback-start-symbolic");
    play.add_css_class("flat");

    let next = gtk4::Button::from_icon_name("media-skip-forward-symbolic");
    next.add_css_class("flat");

    // Wire up mini player controls
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

    center_controls.append(&prev);
    center_controls.append(&play);
    center_controls.append(&next);

    // Right section: expand button
    let expand_btn = gtk4::Button::from_icon_name("go-up-symbolic");
    expand_btn.add_css_class("flat");
    expand_btn.set_tooltip_text(Some("View Now Playing"));

    expand_btn.connect_clicked(glib::clone!(
        #[weak]
        stack,
        move |_| {
            stack.set_visible_child_name("now_playing");
        }
    ));

    controls_row.append(&left_box);
    controls_row.append(&center_controls);
    controls_row.append(&expand_btn);

    container.append(&progress);
    container.append(&controls_row);

    // Wire up mini player to controller
    controller.set_mini_player_elements(progress, cover, title, artist, play);

    container
}
