use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glib::ControlFlow;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::api::{InnertubeClient, SearchResult, StreamInfo};
use crate::config::{
    ICON_HEART_FILLED, ICON_PLAYLIST, ICON_RECENT, MARGIN_MEDIUM, MARGIN_TINY, POLL_INTERVAL_MS,
};
use crate::playback::PlaybackController;
use crate::storage::{Database, Song};
use crate::ui::components::{clear_listbox, cover_widget, section};

/// Builds the library view
pub fn build_library_view(playback: PlaybackController, database: Database) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_vexpand(true);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
    content.set_margin_top(MARGIN_MEDIUM);
    content.set_margin_bottom(MARGIN_MEDIUM);
    content.set_margin_start(MARGIN_MEDIUM);
    content.set_margin_end(MARGIN_MEDIUM);

    let title = gtk4::Label::new(Some("Library"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);

    let database = Rc::new(database);
    let playback = Rc::new(playback);

    // Liked songs section
    let liked_list = gtk4::ListBox::new();
    liked_list.set_selection_mode(gtk4::SelectionMode::None);
    liked_list.add_css_class("boxed-list");
    liked_list.set_activate_on_single_click(true);

    let liked_count = database.get_liked_songs_count();
    let liked_header = format!("Liked Songs ({})", liked_count);

    // Recent plays section
    let recent_list = gtk4::ListBox::new();
    recent_list.set_selection_mode(gtk4::SelectionMode::None);
    recent_list.add_css_class("boxed-list");
    recent_list.set_activate_on_single_click(true);

    // Playlists section
    let playlists_list = gtk4::ListBox::new();
    playlists_list.set_selection_mode(gtk4::SelectionMode::None);
    playlists_list.add_css_class("boxed-list");
    playlists_list.set_activate_on_single_click(true);

    // Playlist detail view (initially hidden)
    let playlist_detail = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    playlist_detail.set_visible(false);

    let playlist_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    let back_btn = gtk4::Button::from_icon_name("go-previous-symbolic");
    back_btn.add_css_class("flat");
    let playlist_title = gtk4::Label::new(None);
    playlist_title.add_css_class("title-2");
    playlist_title.set_hexpand(true);
    playlist_title.set_xalign(0.0);
    let edit_playlist_btn = gtk4::Button::from_icon_name("document-edit-symbolic");
    edit_playlist_btn.add_css_class("flat");
    edit_playlist_btn.add_css_class("circular");
    playlist_header.append(&back_btn);
    playlist_header.append(&playlist_title);
    playlist_header.append(&edit_playlist_btn);

    let playlist_songs_list = gtk4::ListBox::new();
    playlist_songs_list.set_selection_mode(gtk4::SelectionMode::None);
    playlist_songs_list.add_css_class("boxed-list");
    playlist_songs_list.set_activate_on_single_click(true);

    playlist_detail.append(&playlist_header);
    playlist_detail.append(&playlist_songs_list);

    // Main content (will be toggled with playlist detail)
    let main_content = gtk4::Box::new(gtk4::Orientation::Vertical, 24);
    main_content.append(&title);
    main_content.append(&section(&liked_header, Some(ICON_HEART_FILLED), &liked_list));
    main_content.append(&section("Recent Plays", Some(ICON_RECENT), &recent_list));
    main_content.append(&section("Playlists", Some(ICON_PLAYLIST), &playlists_list));

    // Create playlist button
    let add_playlist = gtk4::Button::with_label("Create playlist");
    add_playlist.set_halign(gtk4::Align::Start);
    add_playlist.add_css_class("suggested-action");
    main_content.append(&add_playlist);

    content.append(&main_content);
    content.append(&playlist_detail);

    // Store current playlist ID for detail view
    let current_playlist_id: Rc<Cell<i64>> = Rc::new(Cell::new(0));

    // Load initial data
    load_liked_songs(&liked_list, &database, &playback);
    load_recent_plays(&recent_list, &database, &playback);
    load_playlists(&playlists_list, &database);

    // Handle liked songs row activation (play song)
    wire_liked_songs_playback(&liked_list, &database, &playback);

    // Handle recent plays row activation (play song)
    wire_recent_plays_playback(&recent_list, &database, &playback);

    // Handle playlist row activation (show playlist detail)
    playlists_list.connect_row_activated(glib::clone!(
        #[strong]
        database,
        #[strong]
        playback,
        #[weak]
        main_content,
        #[weak]
        playlist_detail,
        #[weak]
        playlist_title,
        #[weak]
        playlist_songs_list,
        #[strong]
        current_playlist_id,
        move |_, row| {
            let index = row.index();
            if index < 0 {
                return;
            }

            if let Ok(playlists) = database.get_playlists() {
                if let Some(playlist) = playlists.get(index as usize) {
                    current_playlist_id.set(playlist.id);
                    playlist_title.set_text(&playlist.name);

                    load_playlist_songs(&playlist_songs_list, playlist.id, &database, &playback);

                    main_content.set_visible(false);
                    playlist_detail.set_visible(true);
                }
            }
        }
    ));

    // Handle back button from playlist detail
    back_btn.connect_clicked(glib::clone!(
        #[weak]
        main_content,
        #[weak]
        playlist_detail,
        #[weak]
        liked_list,
        #[weak]
        playlists_list,
        #[strong]
        database,
        #[strong]
        playback,
        move |_| {
            main_content.set_visible(true);
            playlist_detail.set_visible(false);
            // Refresh lists when returning
            load_liked_songs(&liked_list, &database, &playback);
            load_playlists(&playlists_list, &database);
        }
    ));

    // Handle create playlist button
    add_playlist.connect_clicked(glib::clone!(
        #[strong]
        database,
        #[weak]
        playlists_list,
        #[weak]
        add_playlist,
        move |_| {
            show_playlist_name_dialog(
                &add_playlist,
                "Create playlist",
                "",
                glib::clone!(
                    #[strong]
                    database,
                    #[weak]
                    playlists_list,
                    move |name| {
                        if database.create_playlist(&name).is_ok() {
                            load_playlists(&playlists_list, &database);
                        }
                    }
                ),
            );
        }
    ));

    // Handle rename playlist button
    edit_playlist_btn.connect_clicked(glib::clone!(
        #[strong]
        database,
        #[weak]
        playlist_title,
        #[strong]
        current_playlist_id,
        #[weak]
        edit_playlist_btn,
        move |_| {
            let playlist_id = current_playlist_id.get();
            if playlist_id <= 0 {
                return;
            }
            let current_name = playlist_title.text().to_string();
            show_playlist_name_dialog(
                &edit_playlist_btn,
                "Rename playlist",
                &current_name,
                glib::clone!(
                    #[strong]
                    database,
                    #[weak]
                    playlist_title,
                    move |name| {
                        if database.rename_playlist(playlist_id, &name).is_ok() {
                            playlist_title.set_text(&name);
                        }
                    }
                ),
            );
        }
    ));

    // Wire playlist songs playback
    wire_playlist_songs_playback(&playlist_songs_list, &current_playlist_id, &database, &playback);

    // Set up periodic refresh of recent plays and liked songs
    setup_refresh_polling(&liked_list, &recent_list, &database, &playback, &main_content);

    scroller.set_child(Some(&content));
    container.append(&scroller);
    container
}

fn load_liked_songs(list: &gtk4::ListBox, database: &Rc<Database>, playback: &Rc<PlaybackController>) {
    clear_listbox(list);

    match database.get_liked_songs() {
        Ok(songs) if !songs.is_empty() => {
            for liked in songs.iter() {
                let row = create_song_row_with_unlike(
                    &liked.song,
                    database.clone(),
                    list.clone(),
                    playback.clone(),
                );
                list.append(&row);
            }
        }
        _ => {
            list.append(&create_empty_state("No liked songs yet", "Like songs to see them here"));
        }
    }
}

fn load_recent_plays(list: &gtk4::ListBox, database: &Rc<Database>, _playback: &Rc<PlaybackController>) {
    clear_listbox(list);

    match database.get_recent_plays() {
        Ok(plays) if !plays.is_empty() => {
            for play in plays.iter() {
                let row = create_song_row(&play.song);
                list.append(&row);
            }
        }
        _ => {
            list.append(&create_empty_state("No recent plays", "Play songs to see them here"));
        }
    }
}

fn load_playlists(list: &gtk4::ListBox, database: &Rc<Database>) {
    clear_listbox(list);

    match database.get_playlists() {
        Ok(playlists) if !playlists.is_empty() => {
            for playlist in playlists.iter() {
                let count = database.get_playlist_song_count(playlist.id);
                let row = create_playlist_row(&playlist.name, count, playlist.id, database.clone(), list.clone());
                list.append(&row);
            }
        }
        _ => {
            list.append(&create_empty_state("No playlists", "Create a playlist to get started"));
        }
    }
}

fn load_playlist_songs(
    list: &gtk4::ListBox,
    playlist_id: i64,
    database: &Rc<Database>,
    playback: &Rc<PlaybackController>,
) {
    clear_listbox(list);

    match database.get_playlist_songs(playlist_id) {
        Ok(songs) if !songs.is_empty() => {
            let total = songs.len();
            for (index, playlist_song) in songs.iter().enumerate() {
                let prev_id = if index > 0 {
                    Some(songs[index - 1].id)
                } else {
                    None
                };
                let next_id = if index + 1 < total {
                    Some(songs[index + 1].id)
                } else {
                    None
                };
                let row = create_playlist_song_row(
                    &playlist_song.song,
                    playlist_song.id,
                    playlist_id,
                    prev_id,
                    next_id,
                    database.clone(),
                    list.clone(),
                    playback.clone(),
                );
                list.append(&row);
            }
        }
        _ => {
            list.append(&create_empty_state("No songs in playlist", "Add songs from search"));
        }
    }
}

fn create_song_row(song: &Song) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let action = adw::ActionRow::new();

    let duration_text = if song.duration.trim().is_empty() {
        "--:--"
    } else {
        &song.duration
    };
    let duration_label = gtk4::Label::new(Some(duration_text));
    duration_label.add_css_class("dim-label");

    action.set_title(&song.title);
    action.set_subtitle(&song.artist);
    action.add_prefix(&cover_widget(song.thumbnail_url.as_deref(), 40));
    action.add_suffix(&duration_label);
    action.set_activatable(true);
    action.add_css_class("song-card");

    row.set_child(Some(&action));
    row
}

fn create_song_row_with_unlike(
    song: &Song,
    database: Rc<Database>,
    list: gtk4::ListBox,
    _playback: Rc<PlaybackController>,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let action = adw::ActionRow::new();

    let duration_text = if song.duration.trim().is_empty() {
        "--:--"
    } else {
        &song.duration
    };
    let duration_label = gtk4::Label::new(Some(duration_text));
    duration_label.add_css_class("dim-label");

    // Unlike button
    let unlike_btn = gtk4::Button::from_icon_name(ICON_HEART_FILLED);
    unlike_btn.add_css_class("flat");
    unlike_btn.add_css_class("circular");
    unlike_btn.add_css_class("liked");

    let video_id = song.video_id.clone();
    let db = database.clone();
    unlike_btn.connect_clicked(glib::clone!(
        #[weak]
        list,
        move |_| {
            if db.unlike_song(&video_id).is_ok() {
                // Refresh the list
                clear_listbox(&list);
                if let Ok(songs) = db.get_liked_songs() {
                    if songs.is_empty() {
                        list.append(&create_empty_state("No liked songs yet", "Like songs to see them here"));
                    } else {
                        for liked in songs.iter() {
                            // We need to recreate rows without the unlike functionality to avoid infinite loop
                            let row = create_song_row(&liked.song);
                            list.append(&row);
                        }
                    }
                }
            }
        }
    ));

    action.set_title(&song.title);
    action.set_subtitle(&song.artist);
    action.add_prefix(&cover_widget(song.thumbnail_url.as_deref(), 40));
    action.add_suffix(&unlike_btn);
    action.add_suffix(&duration_label);
    action.set_activatable(true);
    action.add_css_class("song-card");

    row.set_child(Some(&action));
    row
}

fn create_playlist_row(
    name: &str,
    song_count: i64,
    playlist_id: i64,
    database: Rc<Database>,
    list: gtk4::ListBox,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, MARGIN_TINY);

    let cover_url = database
        .get_playlist_songs(playlist_id)
        .ok()
        .and_then(|songs| songs.first().and_then(|playlist_song| playlist_song.song.thumbnail_url.clone()));
    let cover = cover_widget(cover_url.as_deref(), 40);
    let label = gtk4::Label::new(Some(name));
    label.set_xalign(0.0);
    label.set_hexpand(true);

    let count_label = gtk4::Label::new(Some(&format!("{} songs", song_count)));
    count_label.add_css_class("dim-label");

    // Delete button
    let delete_btn = gtk4::Button::from_icon_name("user-trash-symbolic");
    delete_btn.add_css_class("flat");
    delete_btn.add_css_class("circular");

    let db = database.clone();
    delete_btn.connect_clicked(glib::clone!(
        #[weak]
        list,
        move |button| {
            show_confirm_dialog(
                button,
                "Delete playlist",
                "This will remove the playlist and its songs.",
                glib::clone!(
                    #[weak]
                    list,
                    #[strong]
                    db,
                    move || {
                        if db.delete_playlist(playlist_id).is_ok() {
                            load_playlists(&list, &db);
                        }
                    }
                ),
            );
        }
    ));

    let arrow = gtk4::Image::from_icon_name("go-next-symbolic");
    arrow.add_css_class("dim-label");

    container.append(&cover);
    container.append(&label);
    container.append(&count_label);
    container.append(&delete_btn);
    container.append(&arrow);
    container.set_margin_top(8);
    container.set_margin_bottom(8);
    container.set_margin_start(MARGIN_TINY);
    container.set_margin_end(MARGIN_TINY);

    row.set_child(Some(&container));
    row
}

fn create_playlist_song_row(
    song: &Song,
    song_id: i64,
    playlist_id: i64,
    prev_song_id: Option<i64>,
    next_song_id: Option<i64>,
    database: Rc<Database>,
    list: gtk4::ListBox,
    playback: Rc<PlaybackController>,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let action = adw::ActionRow::new();

    let duration_text = if song.duration.trim().is_empty() {
        "--:--"
    } else {
        &song.duration
    };
    let duration_label = gtk4::Label::new(Some(duration_text));
    duration_label.add_css_class("dim-label");

    // Reorder buttons
    let move_up_btn = gtk4::Button::from_icon_name("go-up-symbolic");
    move_up_btn.add_css_class("flat");
    move_up_btn.add_css_class("circular");
    move_up_btn.set_sensitive(prev_song_id.is_some());

    let move_down_btn = gtk4::Button::from_icon_name("go-down-symbolic");
    move_down_btn.add_css_class("flat");
    move_down_btn.add_css_class("circular");
    move_down_btn.set_sensitive(next_song_id.is_some());

    let db = database.clone();
    let playback_clone = playback.clone();
    move_up_btn.connect_clicked(glib::clone!(
        #[weak]
        list,
        move |_| {
            let Some(prev_id) = prev_song_id else {
                return;
            };
            if db
                .swap_playlist_song_positions(playlist_id, song_id, prev_id)
                .is_ok()
            {
                load_playlist_songs(&list, playlist_id, &db, &playback_clone);
            }
        }
    ));

    let db = database.clone();
    let playback_clone = playback.clone();
    move_down_btn.connect_clicked(glib::clone!(
        #[weak]
        list,
        move |_| {
            let Some(next_id) = next_song_id else {
                return;
            };
            if db
                .swap_playlist_song_positions(playlist_id, song_id, next_id)
                .is_ok()
            {
                load_playlist_songs(&list, playlist_id, &db, &playback_clone);
            }
        }
    ));

    // Remove from playlist button
    let remove_btn = gtk4::Button::from_icon_name("list-remove-symbolic");
    remove_btn.add_css_class("flat");
    remove_btn.add_css_class("circular");

    let db = database.clone();
    remove_btn.connect_clicked(glib::clone!(
        #[weak]
        list,
        #[strong]
        playback,
        move |button| {
            show_confirm_dialog(
                button,
                "Remove from playlist",
                "This song will be removed from the playlist.",
                glib::clone!(
                    #[weak]
                    list,
                    #[strong]
                    playback,
                    #[strong]
                    db,
                    move || {
                        if db.remove_song_from_playlist(playlist_id, song_id).is_ok() {
                            load_playlist_songs(&list, playlist_id, &db, &playback);
                        }
                    }
                ),
            );
        }
    ));

    action.set_title(&song.title);
    action.set_subtitle(&song.artist);
    action.add_prefix(&cover_widget(song.thumbnail_url.as_deref(), 40));
    action.add_suffix(&move_up_btn);
    action.add_suffix(&move_down_btn);
    action.add_suffix(&remove_btn);
    action.add_suffix(&duration_label);
    action.set_activatable(true);
    action.add_css_class("song-card");

    row.set_child(Some(&action));
    row
}

fn create_empty_state(title: &str, subtitle: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);

    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    container.set_margin_top(24);
    container.set_margin_bottom(24);
    container.set_halign(gtk4::Align::Center);

    let title_label = gtk4::Label::new(Some(title));
    title_label.add_css_class("dim-label");

    let subtitle_label = gtk4::Label::new(Some(subtitle));
    subtitle_label.add_css_class("dim-label");
    subtitle_label.add_css_class("caption");

    container.append(&title_label);
    container.append(&subtitle_label);

    row.set_child(Some(&container));
    row
}

fn show_playlist_name_dialog(
    parent: &impl IsA<gtk4::Widget>,
    heading: &str,
    initial: &str,
    on_accept: impl Fn(String) + 'static,
) {
    let parent_window = parent.root().and_downcast::<gtk4::Window>();
    let dialog = gtk4::Dialog::with_buttons(
        Some(heading),
        parent_window.as_ref(),
        gtk4::DialogFlags::MODAL,
        &[("Cancel", gtk4::ResponseType::Cancel), ("Save", gtk4::ResponseType::Ok)],
    );
    dialog.set_default_response(gtk4::ResponseType::Ok);

    let content = dialog.content_area();
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some("Playlist name"));
    entry.set_text(initial);
    content.append(&entry);

    dialog.connect_response(move |dialog: &gtk4::Dialog, response| {
        if response == gtk4::ResponseType::Ok {
            let name = entry.text().to_string();
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                on_accept(trimmed.to_string());
            }
        }
        dialog.close();
    });

    dialog.present();
}

fn show_confirm_dialog(
    parent: &impl IsA<gtk4::Widget>,
    title: &str,
    body: &str,
    on_confirm: impl Fn() + 'static,
) {
    let parent_window = parent.root().and_downcast::<gtk4::Window>();
    let dialog = gtk4::Dialog::with_buttons(
        Some(title),
        parent_window.as_ref(),
        gtk4::DialogFlags::MODAL,
        &[("Cancel", gtk4::ResponseType::Cancel), ("Confirm", gtk4::ResponseType::Ok)],
    );
    dialog.set_default_response(gtk4::ResponseType::Cancel);

    let content = dialog.content_area();
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let label = gtk4::Label::new(Some(body));
    label.set_wrap(true);
    label.set_xalign(0.0);
    content.append(&label);

    dialog.connect_response(move |dialog: &gtk4::Dialog, response| {
        if response == gtk4::ResponseType::Ok {
            on_confirm();
        }
        dialog.close();
    });

    dialog.present();
}

// Playback wiring functions

struct LibraryPlaybackMessage {
    token: u64,
    result: Result<StreamInfo, String>,
    fallback_thumbnail: Option<String>,
}

fn wire_liked_songs_playback(
    list: &gtk4::ListBox,
    database: &Rc<Database>,
    playback: &Rc<PlaybackController>,
) {
    let client = Arc::new(Mutex::new(InnertubeClient::new()));
    let (sender, receiver) = mpsc::channel::<LibraryPlaybackMessage>();
    let receiver = Rc::new(RefCell::new(receiver));
    let playback_token = Rc::new(Cell::new(0u64));

    // Poll for playback results
    glib::timeout_add_local(
        Duration::from_millis(POLL_INTERVAL_MS),
        glib::clone!(
            #[strong]
            receiver,
            #[strong]
            playback,
            #[strong]
            playback_token,
            move || {
                loop {
                    match receiver.borrow().try_recv() {
                        Ok(message) => {
                            if message.token < playback_token.get() {
                                continue;
                            }
                            match message.result {
                                Ok(info) => {
                                    playback.play_stream(&info, message.fallback_thumbnail.as_deref())
                                }
                                Err(error) => playback.show_error(&error),
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return ControlFlow::Break,
                    }
                }
                ControlFlow::Continue
            }
        ),
    );

    list.connect_row_activated(glib::clone!(
        #[strong]
        database,
        #[strong]
        playback,
        #[strong]
        playback_token,
        #[strong]
        client,
        #[strong]
        sender,
        move |_, row| {
            let index = row.index();
            if index < 0 {
                return;
            }

            if let Ok(songs) = database.get_liked_songs() {
                if let Some(liked) = songs.get(index as usize) {
                    let song = &liked.song;
                    let video_id = song.video_id.clone();
                    let thumbnail = song.thumbnail_url.clone();

                    // Set up queue with all liked songs
                    let queue: Vec<SearchResult> = songs.iter().map(|l| SearchResult {
                        video_id: l.song.video_id.clone(),
                        title: l.song.title.clone(),
                        artist: l.song.artist.clone(),
                        duration: l.song.duration.clone(),
                        thumbnail_url: l.song.thumbnail_url.clone(),
                    }).collect();
                    playback.set_queue(queue);
                    playback.set_current_index(index as usize);

                    playback.show_loading("Loading stream...");
                    let token = playback_token.get().saturating_add(1);
                    playback_token.set(token);

                    let client = Arc::clone(&client);
                    let sender = sender.clone();
                    std::thread::spawn(move || {
                        let mut locked = client.lock().expect("innertube client lock");
                        let result = locked.stream_info(&video_id);
                        let _ = sender.send(LibraryPlaybackMessage {
                            token,
                            result,
                            fallback_thumbnail: thumbnail,
                        });
                    });
                }
            }
        }
    ));
}

fn wire_recent_plays_playback(
    list: &gtk4::ListBox,
    database: &Rc<Database>,
    playback: &Rc<PlaybackController>,
) {
    let client = Arc::new(Mutex::new(InnertubeClient::new()));
    let (sender, receiver) = mpsc::channel::<LibraryPlaybackMessage>();
    let receiver = Rc::new(RefCell::new(receiver));
    let playback_token = Rc::new(Cell::new(0u64));

    // Poll for playback results
    glib::timeout_add_local(
        Duration::from_millis(POLL_INTERVAL_MS),
        glib::clone!(
            #[strong]
            receiver,
            #[strong]
            playback,
            #[strong]
            playback_token,
            move || {
                loop {
                    match receiver.borrow().try_recv() {
                        Ok(message) => {
                            if message.token < playback_token.get() {
                                continue;
                            }
                            match message.result {
                                Ok(info) => {
                                    playback.play_stream(&info, message.fallback_thumbnail.as_deref())
                                }
                                Err(error) => playback.show_error(&error),
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return ControlFlow::Break,
                    }
                }
                ControlFlow::Continue
            }
        ),
    );

    list.connect_row_activated(glib::clone!(
        #[strong]
        database,
        #[strong]
        playback,
        #[strong]
        playback_token,
        #[strong]
        client,
        #[strong]
        sender,
        move |_, row| {
            let index = row.index();
            if index < 0 {
                return;
            }

            if let Ok(plays) = database.get_recent_plays() {
                if let Some(play) = plays.get(index as usize) {
                    let song = &play.song;
                    let video_id = song.video_id.clone();
                    let thumbnail = song.thumbnail_url.clone();

                    // Set up queue with recent plays
                    let queue: Vec<SearchResult> = plays.iter().map(|p| SearchResult {
                        video_id: p.song.video_id.clone(),
                        title: p.song.title.clone(),
                        artist: p.song.artist.clone(),
                        duration: p.song.duration.clone(),
                        thumbnail_url: p.song.thumbnail_url.clone(),
                    }).collect();
                    playback.set_queue(queue);
                    playback.set_current_index(index as usize);

                    playback.show_loading("Loading stream...");
                    let token = playback_token.get().saturating_add(1);
                    playback_token.set(token);

                    let client = Arc::clone(&client);
                    let sender = sender.clone();
                    std::thread::spawn(move || {
                        let mut locked = client.lock().expect("innertube client lock");
                        let result = locked.stream_info(&video_id);
                        let _ = sender.send(LibraryPlaybackMessage {
                            token,
                            result,
                            fallback_thumbnail: thumbnail,
                        });
                    });
                }
            }
        }
    ));
}

fn wire_playlist_songs_playback(
    list: &gtk4::ListBox,
    current_playlist_id: &Rc<Cell<i64>>,
    database: &Rc<Database>,
    playback: &Rc<PlaybackController>,
) {
    let client = Arc::new(Mutex::new(InnertubeClient::new()));
    let (sender, receiver) = mpsc::channel::<LibraryPlaybackMessage>();
    let receiver = Rc::new(RefCell::new(receiver));
    let playback_token = Rc::new(Cell::new(0u64));

    // Poll for playback results
    glib::timeout_add_local(
        Duration::from_millis(POLL_INTERVAL_MS),
        glib::clone!(
            #[strong]
            receiver,
            #[strong]
            playback,
            #[strong]
            playback_token,
            move || {
                loop {
                    match receiver.borrow().try_recv() {
                        Ok(message) => {
                            if message.token < playback_token.get() {
                                continue;
                            }
                            match message.result {
                                Ok(info) => {
                                    playback.play_stream(&info, message.fallback_thumbnail.as_deref())
                                }
                                Err(error) => playback.show_error(&error),
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return ControlFlow::Break,
                    }
                }
                ControlFlow::Continue
            }
        ),
    );

    list.connect_row_activated(glib::clone!(
        #[strong]
        database,
        #[strong]
        playback,
        #[strong]
        playback_token,
        #[strong]
        client,
        #[strong]
        sender,
        #[strong]
        current_playlist_id,
        move |_, row| {
            let index = row.index();
            if index < 0 {
                return;
            }

            let playlist_id = current_playlist_id.get();
            if let Ok(songs) = database.get_playlist_songs(playlist_id) {
                if let Some(playlist_song) = songs.get(index as usize) {
                    let song = &playlist_song.song;
                    let video_id = song.video_id.clone();
                    let thumbnail = song.thumbnail_url.clone();

                    // Set up queue with playlist songs
                    let queue: Vec<SearchResult> = songs.iter().map(|ps| SearchResult {
                        video_id: ps.song.video_id.clone(),
                        title: ps.song.title.clone(),
                        artist: ps.song.artist.clone(),
                        duration: ps.song.duration.clone(),
                        thumbnail_url: ps.song.thumbnail_url.clone(),
                    }).collect();
                    playback.set_queue(queue);
                    playback.set_current_index(index as usize);

                    playback.show_loading("Loading stream...");
                    let token = playback_token.get().saturating_add(1);
                    playback_token.set(token);

                    let client = Arc::clone(&client);
                    let sender = sender.clone();
                    std::thread::spawn(move || {
                        let mut locked = client.lock().expect("innertube client lock");
                        let result = locked.stream_info(&video_id);
                        let _ = sender.send(LibraryPlaybackMessage {
                            token,
                            result,
                            fallback_thumbnail: thumbnail,
                        });
                    });
                }
            }
        }
    ));
}

fn setup_refresh_polling(
    _liked_list: &gtk4::ListBox,
    recent_list: &gtk4::ListBox,
    database: &Rc<Database>,
    playback: &Rc<PlaybackController>,
    main_content: &gtk4::Box,
) {
    // Refresh lists every 5 seconds when visible
    glib::timeout_add_local(
        Duration::from_secs(5),
        glib::clone!(
            #[weak_allow_none]
            recent_list,
            #[strong]
            database,
            #[strong]
            playback,
            #[weak_allow_none]
            main_content,
            move || {
                let Some(recent_list) = recent_list else {
                    return ControlFlow::Break;
                };
                let Some(main_content) = main_content else {
                    return ControlFlow::Break;
                };
                // Only refresh if main content is visible (not in playlist detail view)
                if main_content.is_visible() {
                    load_recent_plays(&recent_list, &database, &playback);
                }
                ControlFlow::Continue
            }
        ),
    );
}
