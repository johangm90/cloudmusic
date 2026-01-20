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
use crate::config::{DEBOUNCE_MS, ICON_HEART, ICON_HEART_FILLED, MARGIN_MEDIUM, MARGIN_SMALL, POLL_INTERVAL_MS};
use crate::playback::PlaybackController;
use crate::storage::{Database, Song};
use crate::ui::components::{clear_listbox, cover_widget, loading_row, placeholder_row};

const LOAD_MORE_THRESHOLD: f64 = 200.0;

/// Builds the search view
pub fn build_search_view(playback: PlaybackController, database: Database) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let search_header = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    search_header.set_margin_top(MARGIN_MEDIUM);
    search_header.set_margin_bottom(24);
    search_header.set_margin_start(MARGIN_MEDIUM);
    search_header.set_margin_end(MARGIN_MEDIUM);

    let title = gtk4::Label::new(Some("Search"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search songs, artists, or albums"));
    search_entry.set_hexpand(true);
    search_entry.add_css_class("search-entry");

    search_header.append(&title);
    search_header.append(&search_entry);

    let results_list = gtk4::ListBox::new();
    results_list.set_selection_mode(gtk4::SelectionMode::None);
    results_list.add_css_class("boxed-list");
    results_list.set_activate_on_single_click(true);
    results_list.append(&placeholder_row("Type to search on YouTube"));

    let results_scroller = gtk4::ScrolledWindow::new();
    results_scroller.set_child(Some(&results_list));
    results_scroller.set_vexpand(true);
    results_scroller.set_margin_start(MARGIN_MEDIUM);
    results_scroller.set_margin_end(MARGIN_MEDIUM);
    results_scroller.set_margin_bottom(MARGIN_SMALL);

    wire_search(&search_entry, &results_list, &results_scroller, playback, database);

    container.append(&search_header);
    container.append(&results_scroller);
    container
}

enum SearchMessage {
    Results {
        token: u64,
        results: Vec<SearchResult>,
        continuation: Option<String>,
        append: bool,
    },
    Error {
        token: u64,
        error: String,
        append: bool,
    },
}

impl SearchMessage {
    fn token(&self) -> u64 {
        match self {
            SearchMessage::Results { token, .. } => *token,
            SearchMessage::Error { token, .. } => *token,
        }
    }
}

struct PlaybackMessage {
    token: u64,
    result: Result<StreamInfo, String>,
    fallback_thumbnail: Option<String>,
}

fn wire_search(
    search_entry: &gtk4::SearchEntry,
    results_list: &gtk4::ListBox,
    results_scroller: &gtk4::ScrolledWindow,
    playback: PlaybackController,
    database: Database,
) {
    let client = Arc::new(Mutex::new(InnertubeClient::new()));
    let (sender, receiver) = mpsc::channel::<SearchMessage>();
    let receiver = Rc::new(RefCell::new(receiver));
    let debounce_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let latest_token = Rc::new(Cell::new(0u64));
    let search_results: Rc<RefCell<Vec<SearchResult>>> = Rc::new(RefCell::new(Vec::new()));
    let database = Rc::new(database);
    let continuation_token: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let loading_more = Rc::new(Cell::new(false));
    let current_query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let loading_row_ref: Rc<RefCell<Option<gtk4::ListBoxRow>>> = Rc::new(RefCell::new(None));

    let (play_sender, play_receiver) = mpsc::channel::<PlaybackMessage>();
    let play_receiver = Rc::new(RefCell::new(play_receiver));
    let playback_token = Rc::new(Cell::new(0u64));

    let request_search: Rc<dyn Fn(String, Option<String>, bool, u64)> = {
        let client = Arc::clone(&client);
        let sender = sender.clone();
        Rc::new(move |query: String, continuation: Option<String>, append: bool, token: u64| {
            let client = Arc::clone(&client);
            let sender = sender.clone();
            std::thread::spawn(move || {
                let mut locked = client.lock().expect("innertube client lock");
                let result = locked.search_music_page(&query, continuation.as_deref());
                let message = match result {
                    Ok(page) => SearchMessage::Results {
                        token,
                        results: page.results,
                        continuation: page.continuation,
                        append,
                    },
                    Err(error) => SearchMessage::Error { token, error, append },
                };
                let _ = sender.send(message);
            });
        })
    };

    // Poll for search results
    glib::timeout_add_local(
        Duration::from_millis(POLL_INTERVAL_MS),
        glib::clone!(
            #[weak_allow_none]
            results_list,
            #[strong]
            receiver,
            #[strong]
            latest_token,
            #[strong]
            search_results,
            #[strong]
            playback,
            #[strong]
            database,
            #[strong]
            continuation_token,
            #[strong]
            loading_more,
            #[strong]
            loading_row_ref,
            move || {
                let Some(results_list) = results_list else {
                    return ControlFlow::Break;
                };
                loop {
                    match receiver.borrow().try_recv() {
                        Ok(message) => {
                            if message.token() < latest_token.get() {
                                continue;
                            }
                            let append = matches!(message, SearchMessage::Results { append: true, .. } | SearchMessage::Error { append: true, .. });
                            if let Some(row) = loading_row_ref.borrow_mut().take() {
                                results_list.remove(&row);
                            }
                            loading_more.set(false);
                            if !append {
                                clear_listbox(&results_list);
                                search_results.borrow_mut().clear();
                            }
                            match message {
                                SearchMessage::Results { results, continuation, append, .. } => {
                                    *continuation_token.borrow_mut() = continuation;
                                    let mut stored = search_results.borrow_mut();
                                    if !append {
                                        stored.clear();
                                    }
                                    stored.extend(results.iter().cloned());
                                    playback.set_queue(stored.clone());
                                    if stored.is_empty() {
                                        results_list.append(&placeholder_row("No results"));
                                    } else {
                                        let parent_widget = results_list.clone().upcast::<gtk4::Widget>();
                                        for item in results.iter() {
                                            let row = create_search_result_row(
                                                &database,
                                                item,
                                                &parent_widget,
                                            );
                                            results_list.append(&row);
                                        }
                                    }
                                }
                                SearchMessage::Error { error, .. } => {
                                    results_list.append(&placeholder_row(&error));
                                }
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => break,
                    }
                }
                ControlFlow::Continue
            }
        ),
    );

    // Poll for playback results
    glib::timeout_add_local(
        Duration::from_millis(POLL_INTERVAL_MS),
        glib::clone!(
            #[strong]
            play_receiver,
            #[strong]
            playback,
            #[strong]
            playback_token,
            move || {
                loop {
                    match play_receiver.borrow().try_recv() {
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
                        Err(mpsc::TryRecvError::Disconnected) => break,
                    }
                }
                ControlFlow::Continue
            }
        ),
    );

    let request_play: Rc<dyn Fn(SearchResult, u64)> = {
        let client = Arc::clone(&client);
        let sender = play_sender.clone();
        Rc::new(move |item: SearchResult, token: u64| {
            let client = Arc::clone(&client);
            let sender = sender.clone();
            std::thread::spawn(move || {
                let mut locked = client.lock().expect("innertube client lock");
                let result = locked.stream_info(&item.video_id);
                let message = PlaybackMessage {
                    token,
                    result,
                    fallback_thumbnail: item.thumbnail_url.clone(),
                };
                let _ = sender.send(message);
            });
        })
    };

    // Handle row activation in results list
    results_list.connect_row_activated(glib::clone!(
        #[strong]
        search_results,
        #[strong]
        playback,
        #[strong]
        playback_token,
        #[strong]
        request_play,
        move |_, row| {
            let index = row.index();
            if index < 0 {
                return;
            }
            let index = index as usize;
            let item = search_results.borrow().get(index).cloned();
            let Some(item) = item else {
                return;
            };

            playback.show_loading("Loading stream...");
            let token = playback_token.get().saturating_add(1);
            playback_token.set(token);
            playback.set_current_index(index);
            request_play.as_ref()(item, token);
        }
    ));

    let vadjustment = results_scroller.vadjustment();
    vadjustment.connect_value_changed(glib::clone!(
        #[strong]
        latest_token,
        #[strong]
        continuation_token,
        #[strong]
        current_query,
        #[strong]
        loading_more,
        #[strong]
        request_search,
        #[strong]
        results_list,
        #[strong]
        loading_row_ref,
        move |adjustment: &gtk4::Adjustment| {
            if loading_more.get() {
                return;
            }
            let upper = adjustment.upper();
            let page = adjustment.page_size();
            let value = adjustment.value();
            if upper - (value + page) > LOAD_MORE_THRESHOLD {
                return;
            }
            let query = current_query.borrow().clone();
            if query.is_empty() {
                return;
            }
            let continuation = continuation_token.borrow().clone();
            let Some(token) = continuation else {
                return;
            };
            loading_more.set(true);
            if loading_row_ref.borrow().is_none() {
                let row = loading_row("Loading more...");
                results_list.append(&row);
                *loading_row_ref.borrow_mut() = Some(row);
            }
            request_search(query, Some(token), true, latest_token.get());
        }
    ));

    // Wire up play button
    if let Some(play_btn) = playback.play_button() {
        play_btn.connect_clicked(glib::clone!(
            #[strong]
            playback,
            move |_| {
                playback.toggle_play_pause();
            }
        ));
    }

    // Wire up previous button
    if let Some(prev_btn) = playback.prev_button() {
        prev_btn.connect_clicked(glib::clone!(
            #[strong]
            playback,
            #[strong]
            playback_token,
            #[strong]
            request_play,
            move |_| {
                let Some(item) = playback.shift_index(-1) else {
                    return;
                };
                playback.show_loading("Loading previous...");
                let token = playback_token.get().saturating_add(1);
                playback_token.set(token);
                request_play.as_ref()(item, token);
            }
        ));
    }

    // Wire up next button
    if let Some(next_btn) = playback.next_button() {
        next_btn.connect_clicked(glib::clone!(
            #[strong]
            playback,
            #[strong]
            playback_token,
            #[strong]
            request_play,
            move |_| {
                let Some(item) = playback.shift_index(1) else {
                    return;
                };
                playback.show_loading("Loading next...");
                let token = playback_token.get().saturating_add(1);
                playback_token.set(token);
                request_play.as_ref()(item, token);
            }
        ));
    }

    // Wire up queue list row activation
    if let Some(queue) = playback.queue_list() {
        queue.connect_row_activated(glib::clone!(
            #[strong]
            playback,
            #[strong]
            playback_token,
            #[strong]
            request_play,
            move |_, row| {
                let index = row.index();
                if index < 0 {
                    return;
                }
                let index = index as usize;
                playback.set_current_index(index);
                let Some(item) = playback.current_item() else {
                    return;
                };
                playback.show_loading("Loading stream...");
                let token = playback_token.get().saturating_add(1);
                playback_token.set(token);
                request_play.as_ref()(item, token);
            }
        ));
    }

    // Set up prev/next callbacks for mini player
    playback.set_prev_callback(glib::clone!(
        #[strong]
        playback,
        #[strong]
        playback_token,
        #[strong]
        request_play,
        move || {
            let Some(item) = playback.shift_index(-1) else {
                return;
            };
            playback.show_loading("Loading previous...");
            let token = playback_token.get().saturating_add(1);
            playback_token.set(token);
            request_play.as_ref()(item, token);
        }
    ));

    playback.set_next_callback(glib::clone!(
        #[strong]
        playback,
        #[strong]
        playback_token,
        #[strong]
        request_play,
        move || {
            let Some(item) = playback.shift_index(1) else {
                return;
            };
            playback.show_loading("Loading next...");
            let token = playback_token.get().saturating_add(1);
            playback_token.set(token);
            request_play.as_ref()(item, token);
        }
    ));

    // Handle search input with debouncing
    search_entry.connect_changed(glib::clone!(
        #[weak]
        search_entry,
        #[weak]
        results_list,
        #[strong]
        debounce_id,
        #[strong]
        latest_token,
        #[strong]
        continuation_token,
        #[strong]
        current_query,
        #[strong]
        loading_more,
        #[strong]
        request_search,
        #[strong]
        loading_row_ref,
        move |_| {
            debounce_id.borrow_mut().take();

            let query = search_entry.text().to_string();
            let token = latest_token.get().saturating_add(1);
            latest_token.set(token);
            let continuation_token = continuation_token.clone();
            let current_query = current_query.clone();
            let loading_more = loading_more.clone();
            let request_search = request_search.clone();
            let loading_row_ref = loading_row_ref.clone();
            let id = glib::timeout_add_local(Duration::from_millis(DEBOUNCE_MS), move || {
                let trimmed = query.trim().to_string();
                if trimmed.is_empty() {
                    clear_listbox(&results_list);
                    results_list.append(&placeholder_row("Type to search on YouTube"));
                    *continuation_token.borrow_mut() = None;
                    *current_query.borrow_mut() = String::new();
                    if let Some(row) = loading_row_ref.borrow_mut().take() {
                        results_list.remove(&row);
                    }
                    loading_more.set(false);
                    return ControlFlow::Break;
                }

                // Show loading indicator
                clear_listbox(&results_list);
                results_list.append(&loading_row("Searching..."));
                *continuation_token.borrow_mut() = None;
                *current_query.borrow_mut() = trimmed.clone();
                loading_more.set(true);
                if let Some(row) = loading_row_ref.borrow_mut().take() {
                    results_list.remove(&row);
                }
                request_search(trimmed, None, false, token);

                ControlFlow::Break
            });

            *debounce_id.borrow_mut() = Some(id);
        }
    ));
}

/// Creates a like button for a song
fn create_like_button(database: &Rc<Database>, item: &SearchResult) -> gtk4::Button {
    let is_liked = database.is_song_liked(&item.video_id);

    let btn = gtk4::Button::new();
    btn.set_icon_name(if is_liked { ICON_HEART_FILLED } else { ICON_HEART });
    btn.add_css_class("flat");
    btn.add_css_class("circular");
    if is_liked {
        btn.add_css_class("liked");
    }

    let db = database.clone();
    let video_id = item.video_id.clone();
    let title = item.title.clone();
    let artist = item.artist.clone();
    let duration = item.duration.clone();
    let thumbnail_url = item.thumbnail_url.clone();

    btn.connect_clicked(move |button| {
        let currently_liked = db.is_song_liked(&video_id);
        if currently_liked {
            // Unlike the song
            if db.unlike_song(&video_id).is_ok() {
                button.set_icon_name(ICON_HEART);
                button.remove_css_class("liked");
            }
        } else {
            // Like the song
            let song = Song {
                video_id: video_id.clone(),
                title: title.clone(),
                artist: artist.clone(),
                duration: duration.clone(),
                thumbnail_url: thumbnail_url.clone(),
            };
            if db.like_song(&song).is_ok() {
                button.set_icon_name(ICON_HEART_FILLED);
                button.add_css_class("liked");
            }
        }
    });

    btn
}

fn create_add_to_playlist_button(
    database: &Rc<Database>,
    item: &SearchResult,
    parent: &gtk4::Widget,
) -> gtk4::Button {
    let btn = gtk4::Button::from_icon_name("list-add-symbolic");
    btn.add_css_class("flat");
    btn.add_css_class("circular");

    let db = database.clone();
    let item = item.clone();
    let parent = parent.clone();
    btn.connect_clicked(move |_| {
        show_add_to_playlist_dialog(&parent, &db, &item);
    });

    btn
}

fn create_search_result_row(
    database: &Rc<Database>,
    item: &SearchResult,
    parent: &gtk4::Widget,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let action = adw::ActionRow::new();

    let duration_text = if item.duration.trim().is_empty() {
        "--:--"
    } else {
        &item.duration
    };
    let duration_label = gtk4::Label::new(Some(duration_text));
    duration_label.add_css_class("dim-label");

    let like_btn = create_like_button(database, item);
    let add_btn = create_add_to_playlist_button(database, item, parent);

    action.set_title(&item.title);
    action.set_subtitle(&item.artist);
    action.add_prefix(&cover_widget(item.thumbnail_url.as_deref(), 40));
    action.add_suffix(&add_btn);
    action.add_suffix(&like_btn);
    action.add_suffix(&duration_label);
    action.set_activatable(true);
    action.add_css_class("song-card");

    row.set_child(Some(&action));
    row
}

fn show_add_to_playlist_dialog(
    parent: &gtk4::Widget,
    database: &Rc<Database>,
    item: &SearchResult,
) {
    let playlists = database.get_playlists().unwrap_or_default();
    let parent_window = parent.root().and_downcast::<gtk4::Window>();

    if playlists.is_empty() {
        let dialog = gtk4::Dialog::with_buttons(
            Some("No playlists yet"),
            parent_window.as_ref(),
            gtk4::DialogFlags::MODAL,
            &[("OK", gtk4::ResponseType::Ok)],
        );
        dialog.set_default_response(gtk4::ResponseType::Ok);

        let content = dialog.content_area();
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let label = gtk4::Label::new(Some("Create a playlist in Library first."));
        label.set_wrap(true);
        label.set_xalign(0.0);
        content.append(&label);

        dialog.connect_response(|dialog: &gtk4::Dialog, _| dialog.close());
        dialog.present();
        return;
    }

    let names: Vec<String> = playlists.iter().map(|p| p.name.clone()).collect();
    let name_refs: Vec<&str> = names.iter().map(|name| name.as_str()).collect();
    let dropdown = gtk4::DropDown::from_strings(&name_refs);

    let dialog = gtk4::Dialog::with_buttons(
        Some("Add to playlist"),
        parent_window.as_ref(),
        gtk4::DialogFlags::MODAL,
        &[("Cancel", gtk4::ResponseType::Cancel), ("Add", gtk4::ResponseType::Ok)],
    );
    dialog.set_default_response(gtk4::ResponseType::Ok);

    let content = dialog.content_area();
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&dropdown);

    let db = database.clone();
    let item = item.clone();
    dialog.connect_response(move |dialog: &gtk4::Dialog, response| {
        if response == gtk4::ResponseType::Ok {
            let index = dropdown.selected() as usize;
            if let Some(playlist) = playlists.get(index) {
                let song = Song {
                    video_id: item.video_id.clone(),
                    title: item.title.clone(),
                    artist: item.artist.clone(),
                    duration: item.duration.clone(),
                    thumbnail_url: item.thumbnail_url.clone(),
                };
                let _ = db.add_song_to_playlist(playlist.id, &song);
            }
        }
        dialog.close();
    });

    dialog.present();
}
