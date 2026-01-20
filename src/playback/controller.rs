use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use glib::ControlFlow;
use gtk4::prelude::*;

use crate::api::{LyricLine, SearchResult, StreamInfo};
use crate::config::{DEFAULT_COVER_PATH, POLL_INTERVAL_MS};
use crate::storage::{Database, Song};
use crate::ui::components::{clear_listbox, load_image_async, load_image_async_with_callback, song_card_row, RgbColor};

#[derive(Clone)]
pub struct PlaybackController {
    media: Rc<RefCell<Option<gtk4::MediaFile>>>,
    title: Rc<RefCell<Option<gtk4::Label>>>,
    artist: Rc<RefCell<Option<gtk4::Label>>>,
    progress: Rc<RefCell<Option<gtk4::Scale>>>,
    mini_progress: Rc<RefCell<Option<gtk4::Scale>>>,
    current_time: Rc<RefCell<Option<gtk4::Label>>>,
    total_time: Rc<RefCell<Option<gtk4::Label>>>,
    media_token: Rc<Cell<u64>>,
    cover_token: Rc<Cell<u64>>,
    queue: Rc<RefCell<Vec<SearchResult>>>,
    current_index: Rc<Cell<i32>>,
    queue_list: Rc<RefCell<Option<gtk4::ListBox>>>,
    queue_rows: Rc<RefCell<Vec<gtk4::ListBoxRow>>>,
    queue_scroller: Rc<RefCell<Option<gtk4::ScrolledWindow>>>,
    play_button: Rc<RefCell<Option<gtk4::Button>>>,
    prev_button: Rc<RefCell<Option<gtk4::Button>>>,
    next_button: Rc<RefCell<Option<gtk4::Button>>>,
    cover: Rc<RefCell<Option<gtk4::Image>>>,
    mini_cover: Rc<RefCell<Option<gtk4::Image>>>,
    mini_title: Rc<RefCell<Option<gtk4::Label>>>,
    mini_artist: Rc<RefCell<Option<gtk4::Label>>>,
    mini_player: Rc<RefCell<Option<gtk4::Box>>>,
    mini_play_button: Rc<RefCell<Option<gtk4::Button>>>,
    has_played: Rc<Cell<bool>>,
    on_prev_callback: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_next_callback: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    // Visualizer elements
    ring1: Rc<RefCell<Option<gtk4::DrawingArea>>>,
    ring2: Rc<RefCell<Option<gtk4::DrawingArea>>>,
    ring3: Rc<RefCell<Option<gtk4::DrawingArea>>>,
    background: Rc<RefCell<Option<Rc<RefCell<gtk4::Box>>>>>,
    is_playing: Rc<Cell<bool>>,
    lyrics_lines: Rc<RefCell<Vec<LyricLine>>>,
    lyrics_rows: Rc<RefCell<Vec<gtk4::ListBoxRow>>>,
    lyrics_list: Rc<RefCell<Option<gtk4::ListBox>>>,
    lyrics_scroller: Rc<RefCell<Option<gtk4::ScrolledWindow>>>,
    current_lyric_index: Rc<Cell<i32>>,
    is_now_playing_visible: Rc<Cell<bool>>,
    // Database for recent plays and liked songs
    database: Rc<RefCell<Option<Database>>>,
}

impl PlaybackController {
    pub fn new() -> Self {
        Self {
            media: Rc::new(RefCell::new(None)),
            title: Rc::new(RefCell::new(None)),
            artist: Rc::new(RefCell::new(None)),
            progress: Rc::new(RefCell::new(None)),
            mini_progress: Rc::new(RefCell::new(None)),
            current_time: Rc::new(RefCell::new(None)),
            total_time: Rc::new(RefCell::new(None)),
            media_token: Rc::new(Cell::new(0)),
            cover_token: Rc::new(Cell::new(0)),
            queue: Rc::new(RefCell::new(Vec::new())),
            current_index: Rc::new(Cell::new(-1)),
            queue_list: Rc::new(RefCell::new(None)),
            queue_rows: Rc::new(RefCell::new(Vec::new())),
            queue_scroller: Rc::new(RefCell::new(None)),
            play_button: Rc::new(RefCell::new(None)),
            prev_button: Rc::new(RefCell::new(None)),
            next_button: Rc::new(RefCell::new(None)),
            cover: Rc::new(RefCell::new(None)),
            mini_cover: Rc::new(RefCell::new(None)),
            mini_title: Rc::new(RefCell::new(None)),
            mini_artist: Rc::new(RefCell::new(None)),
            mini_player: Rc::new(RefCell::new(None)),
            mini_play_button: Rc::new(RefCell::new(None)),
            has_played: Rc::new(Cell::new(false)),
            on_prev_callback: Rc::new(RefCell::new(None)),
            on_next_callback: Rc::new(RefCell::new(None)),
            ring1: Rc::new(RefCell::new(None)),
            ring2: Rc::new(RefCell::new(None)),
            ring3: Rc::new(RefCell::new(None)),
            background: Rc::new(RefCell::new(None)),
            is_playing: Rc::new(Cell::new(false)),
            lyrics_lines: Rc::new(RefCell::new(Vec::new())),
            lyrics_rows: Rc::new(RefCell::new(Vec::new())),
            lyrics_list: Rc::new(RefCell::new(None)),
            lyrics_scroller: Rc::new(RefCell::new(None)),
            current_lyric_index: Rc::new(Cell::new(-1)),
            is_now_playing_visible: Rc::new(Cell::new(false)),
            database: Rc::new(RefCell::new(None)),
        }
    }

    pub fn set_database(&self, db: Database) {
        *self.database.borrow_mut() = Some(db);
    }

    pub fn set_now_playing_visible(&self, visible: bool) {
        self.is_now_playing_visible.set(visible);
    }

    pub fn set_visualizer_elements(
        &self,
        ring1: gtk4::DrawingArea,
        ring2: gtk4::DrawingArea,
        ring3: gtk4::DrawingArea,
        background: Rc<RefCell<gtk4::Box>>,
    ) {
        *self.ring1.borrow_mut() = Some(ring1);
        *self.ring2.borrow_mut() = Some(ring2);
        *self.ring3.borrow_mut() = Some(ring3);
        *self.background.borrow_mut() = Some(background);
    }

    fn start_visualizer(&self) {
        self.is_playing.set(true);
        // Add playing class to rings for CSS animation
        if let Some(ref ring) = *self.ring1.borrow() {
            ring.add_css_class("playing");
        }
        if let Some(ref ring) = *self.ring2.borrow() {
            ring.add_css_class("playing");
        }
        if let Some(ref ring) = *self.ring3.borrow() {
            ring.add_css_class("playing");
        }
        
        // Start vinyl spinning animation
        if let Some(ref cover) = *self.cover.borrow() {
            cover.add_css_class("spinning");
        }
        
        // Start beat detection for reactive visualizer
        self.start_beat_detection();
    }

    fn stop_visualizer(&self) {
        self.is_playing.set(false);
        // Remove playing class from rings
        if let Some(ref ring) = *self.ring1.borrow() {
            ring.remove_css_class("playing");
            ring.remove_css_class("beat-pulse");
        }
        if let Some(ref ring) = *self.ring2.borrow() {
            ring.remove_css_class("playing");
            ring.remove_css_class("beat-pulse");
        }
        if let Some(ref ring) = *self.ring3.borrow() {
            ring.remove_css_class("playing");
            ring.remove_css_class("beat-pulse");
        }
        
        // Stop vinyl spinning
        if let Some(ref cover) = *self.cover.borrow() {
            cover.remove_css_class("spinning");
        }
    }

    fn start_beat_detection(&self) {
        let is_playing = self.is_playing.clone();
        let ring1 = self.ring1.clone();
        let ring2 = self.ring2.clone();
        let ring3 = self.ring3.clone();
        let media_token = self.media_token.clone();
        let token = media_token.get();

        // Beat detection: add pulse effect periodically synced to typical music tempo
        // Average music BPM ~120, so beat every ~500ms
        glib::timeout_add_local(Duration::from_millis(520), move || {
            if media_token.get() != token || !is_playing.get() {
                return ControlFlow::Break;
            }

            // Add beat pulse class temporarily
            if let Some(ref ring) = *ring1.borrow() {
                ring.add_css_class("beat-pulse");
            }
            if let Some(ref ring) = *ring2.borrow() {
                ring.add_css_class("beat-pulse");
            }
            if let Some(ref ring) = *ring3.borrow() {
                ring.add_css_class("beat-pulse");
            }

            // Remove pulse class after short duration
            let ring1_clone = ring1.clone();
            let ring2_clone = ring2.clone();
            let ring3_clone = ring3.clone();
            glib::timeout_add_local(Duration::from_millis(120), move || {
                if let Some(ref ring) = *ring1_clone.borrow() {
                    ring.remove_css_class("beat-pulse");
                }
                if let Some(ref ring) = *ring2_clone.borrow() {
                    ring.remove_css_class("beat-pulse");
                }
                if let Some(ref ring) = *ring3_clone.borrow() {
                    ring.remove_css_class("beat-pulse");
                }
                ControlFlow::Break
            });

            ControlFlow::Continue
        });
    }

    pub fn set_prev_callback<F: Fn() + 'static>(&self, callback: F) {
        *self.on_prev_callback.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_next_callback<F: Fn() + 'static>(&self, callback: F) {
        *self.on_next_callback.borrow_mut() = Some(Box::new(callback));
    }

    pub fn play_previous(&self) {
        if let Some(ref callback) = *self.on_prev_callback.borrow() {
            callback();
        }
    }

    pub fn play_next(&self) {
        if let Some(ref callback) = *self.on_next_callback.borrow() {
            callback();
        }
    }

    pub fn seek(&self, position_secs: f64) {
        let binding = self.media.borrow();
        if let Some(ref media) = *binding {
            let position_usecs = (position_secs * 1_000_000.0) as i64;
            media.seek(position_usecs);
        }
    }

    pub fn set_mini_player_widget(&self, widget: gtk4::Box) {
        *self.mini_player.borrow_mut() = Some(widget);
    }

    pub fn has_played(&self) -> bool {
        self.has_played.get()
    }

    pub fn show_mini_player(&self) {
        if let Some(ref mini_player) = *self.mini_player.borrow() {
            mini_player.set_visible(true);
        }
    }

    pub fn hide_mini_player(&self) {
        if let Some(ref mini_player) = *self.mini_player.borrow() {
            mini_player.set_visible(false);
        }
    }

    pub fn set_ui_elements(
        &self,
        title: gtk4::Label,
        artist: gtk4::Label,
        progress: gtk4::Scale,
        queue_list: gtk4::ListBox,
        play_button: gtk4::Button,
        prev_button: gtk4::Button,
        next_button: gtk4::Button,
        cover: gtk4::Image,
        current_time: Option<gtk4::Label>,
        total_time: Option<gtk4::Label>,
    ) {
        *self.title.borrow_mut() = Some(title);
        *self.artist.borrow_mut() = Some(artist);
        *self.progress.borrow_mut() = Some(progress.clone());
        *self.queue_list.borrow_mut() = Some(queue_list);
        *self.play_button.borrow_mut() = Some(play_button);
        *self.prev_button.borrow_mut() = Some(prev_button);
        *self.next_button.borrow_mut() = Some(next_button);
        *self.cover.borrow_mut() = Some(cover);
        *self.current_time.borrow_mut() = current_time;
        *self.total_time.borrow_mut() = total_time;

        self.setup_progress_updates(&progress);
    }

    pub fn set_mini_player_elements(
        &self,
        progress: gtk4::Scale,
        cover: gtk4::Image,
        title: gtk4::Label,
        artist: gtk4::Label,
        play_button: gtk4::Button,
    ) {
        *self.mini_progress.borrow_mut() = Some(progress);
        *self.mini_cover.borrow_mut() = Some(cover);
        *self.mini_title.borrow_mut() = Some(title);
        *self.mini_artist.borrow_mut() = Some(artist);
        *self.mini_play_button.borrow_mut() = Some(play_button);
    }

    pub fn set_lyrics_elements(&self, lyrics_list: gtk4::ListBox, lyrics_scroller: gtk4::ScrolledWindow) {
        *self.lyrics_list.borrow_mut() = Some(lyrics_list);
        *self.lyrics_scroller.borrow_mut() = Some(lyrics_scroller);
        self.set_lyrics_lines(None, "Lyrics will appear here");
    }

    pub fn set_queue_scroller(&self, scroller: gtk4::ScrolledWindow) {
        *self.queue_scroller.borrow_mut() = Some(scroller);
    }

    fn set_lyrics_lines(&self, lines: Option<&[LyricLine]>, placeholder: &str) {
        self.current_lyric_index.set(-1);
        self.lyrics_lines.borrow_mut().clear();
        self.lyrics_rows.borrow_mut().clear();

        let Some(ref list) = *self.lyrics_list.borrow() else {
            return;
        };

        clear_listbox(list);

        if let Some(lines) = lines {
            if !lines.is_empty() {
                self.lyrics_lines.borrow_mut().extend_from_slice(lines);
                for line in lines {
                    let row = build_lyrics_row(&line.text);
                    list.append(&row);
                    self.lyrics_rows.borrow_mut().push(row);
                }
                return;
            }
        }

        let row = build_lyrics_placeholder_row(placeholder);
        list.append(&row);
    }

    fn setup_progress_updates(&self, progress: &gtk4::Scale) {
        // Initial setup - just clone references for the polling
        let _ = progress; // Not used in initial setup anymore
    }

    fn start_progress_polling(&self) {
        let media = self.media.clone();
        let media_token = self.media_token.clone();
        let token = media_token.get();
        let current_time = self.current_time.clone();
        let total_time = self.total_time.clone();
        let progress = self.progress.clone();
        let mini_progress = self.mini_progress.clone();
        let lyrics_lines = self.lyrics_lines.clone();
        let lyrics_rows = self.lyrics_rows.clone();
        let lyrics_scroller = self.lyrics_scroller.clone();
        let current_lyric_index = self.current_lyric_index.clone();

        glib::timeout_add_local(Duration::from_millis(POLL_INTERVAL_MS), move || {
            if media_token.get() != token {
                return ControlFlow::Break;
            }

            let binding = media.borrow();
            let Some(media_file) = binding.as_ref() else {
                return ControlFlow::Continue;
            };

            let duration = media_file.duration();
            let timestamp = media_file.timestamp();

            if duration > 0 {
                let duration_secs = duration as f64 / 1_000_000.0;

                if let Some(ref prog) = *progress.borrow() {
                    prog.set_range(0.0, duration_secs);
                    prog.set_sensitive(true);
                }

                if let Some(ref mini) = *mini_progress.borrow() {
                    mini.set_range(0.0, duration_secs);
                }

                if let Some(ref label) = *total_time.borrow() {
                    label.set_text(&format_duration(duration_secs as i64));
                }
            }

            if timestamp >= 0 {
                let seconds = timestamp as f64 / 1_000_000.0;

                if let Some(ref prog) = *progress.borrow() {
                    prog.set_value(seconds);
                }

                if let Some(ref mini) = *mini_progress.borrow() {
                    mini.set_value(seconds);
                }

                if let Some(ref label) = *current_time.borrow() {
                    label.set_text(&format_duration(seconds as i64));
                }

                update_lyrics_position(
                    seconds,
                    &lyrics_lines,
                    &lyrics_rows,
                    &lyrics_scroller,
                    &current_lyric_index,
                );
            }

            ControlFlow::Continue
        });
    }

    pub fn play_stream(&self, info: &StreamInfo, fallback_thumbnail: Option<&str>) {
        // Enable controls after loading
        self.enable_controls();
        
        if let Some(media) = self.media.borrow_mut().take() {
            media.pause();
        }

        let token = self.media_token.get().saturating_add(1);
        self.media_token.set(token);

        // Mark that playback has started and show mini player (only if not on now playing view)
        self.has_played.set(true);
        if !self.is_now_playing_visible.get() {
            self.show_mini_player();
        }

        // Record this play in recent plays
        if let Some(ref db) = *self.database.borrow() {
            if let Some(current) = self.current_item() {
                let song = Song {
                    video_id: current.video_id.clone(),
                    title: info.title.clone(),
                    artist: info.artist.clone(),
                    duration: current.duration.clone(),
                    thumbnail_url: info.thumbnail_url.clone()
                        .or_else(|| current.thumbnail_url.clone())
                        .or_else(|| fallback_thumbnail.map(String::from)),
                };
                let _ = db.add_recent_play(&song);
            }
        }

        if let Some(ref progress) = *self.progress.borrow() {
            progress.set_sensitive(false);
            progress.set_value(0.0);
        }

        if let Some(ref mini_progress) = *self.mini_progress.borrow() {
            mini_progress.set_value(0.0);
        }

        let cover_token = self.cover_token.get().saturating_add(1);
        self.cover_token.set(cover_token);

        let file = gtk4::gio::File::for_uri(&info.url);
        let media = gtk4::MediaFile::for_file(&file);

        // Listen for when the song ends to auto-play next
        let controller = self.clone();
        let end_token = token;
        media.connect_ended_notify(move |media| {
            if media.is_ended() && controller.media_token.get() == end_token {
                // Song ended, play next
                controller.play_next();
            }
        });

        media.play();
        *self.media.borrow_mut() = Some(media);

        // Start progress updates with new token
        self.start_progress_polling();

        // Start visualizer animation
        self.start_visualizer();

        // Update now playing view
        if let Some(ref title) = *self.title.borrow() {
            title.set_text(&info.title);
        }
        if let Some(ref artist) = *self.artist.borrow() {
            artist.set_text(&info.artist);
        }

        // Update mini player
        if let Some(ref title) = *self.mini_title.borrow() {
            title.set_text(&info.title);
        }
        if let Some(ref artist) = *self.mini_artist.borrow() {
            artist.set_text(&info.artist);
        }

        self.set_lyrics_lines(info.lyrics.as_deref(), "No synced lyrics available");

        // Update both play buttons to pause icon
        if let Some(ref play_button) = *self.play_button.borrow() {
            play_button.set_icon_name("media-playback-pause-symbolic");
        }
        if let Some(ref play_button) = *self.mini_play_button.borrow() {
            play_button.set_icon_name("media-playback-pause-symbolic");
        }

        let chosen_thumbnail = info.thumbnail_url.as_deref().or(fallback_thumbnail);

        // Update main cover with color callback
        if let Some(ref cover) = *self.cover.borrow() {
            cover.set_from_file(Some(DEFAULT_COVER_PATH));
            if let Some(url) = chosen_thumbnail {
                if url.starts_with("http://") || url.starts_with("https://") {
                    let background = self.background.clone();
                    let progress = self.progress.clone();
                    let play_button = self.play_button.clone();
                    let prev_button = self.prev_button.clone();
                    let next_button = self.next_button.clone();
                    let ring1 = self.ring1.clone();
                    let ring2 = self.ring2.clone();
                    let ring3 = self.ring3.clone();
                    
                    load_image_async_with_callback(
                        cover.clone(),
                        url.to_string(),
                        Some((self.cover_token.clone(), cover_token)),
                        Some(move |color: RgbColor| {
                            // Apply color to background
                            if let Some(ref bg_ref) = *background.borrow() {
                                if let Ok(bg) = bg_ref.try_borrow() {
                                    let accent = color.with_saturation(1.25);
                                    let accent = if accent.is_light() {
                                        accent.mix(RgbColor { r: 0, g: 0, b: 0 }, 0.1)
                                    } else {
                                        accent.mix(RgbColor { r: 255, g: 255, b: 255 }, 0.1)
                                    };
                                    let accent_soft = accent.mix(RgbColor { r: 255, g: 255, b: 255 }, 0.35);
                                    let accent_deep = accent.mix(RgbColor { r: 0, g: 0, b: 0 }, 0.45);
                                    let accent_glow = accent.mix(RgbColor { r: 255, g: 255, b: 255 }, 0.2);
                                    let title_color = accent.text_rgba(0.95);
                                    let subtitle_color = accent.text_rgba(0.7);
                                    let panel_text = accent.text_rgba(0.78);

                                    // Create CSS provider for dynamic theming
                                    let css = format!(
                                        ".now-playing-background {{
                                            background:
                                                radial-gradient(120% 80% at 50% 8%, {} 0%, {} 35%, @window_bg_color 75%),
                                                linear-gradient(180deg, {} 0%, @window_bg_color 60%);
                                            transition: background 800ms cubic-bezier(0.4, 0, 0.2, 1);
                                        }}

                                        .side-panel {{
                                            background: linear-gradient(180deg, {} 0%, alpha(@card_bg_color, 0.35) 65%);
                                            border-color: {} !important;
                                            box-shadow: 0 10px 30px {} !important;
                                        }}
                                        
                                        .cover-container {{
                                            box-shadow:
                                                0 12px 48px rgba(0, 0, 0, 0.4),
                                                0 0 0 6px {},
                                                0 0 0 1px {},
                                                0 0 24px {};
                                        }}
                                        
                                        .now-playing-title {{
                                            color: {};
                                        }}
                                        
                                        .now-playing-artist {{
                                            color: {};
                                        }}
                                        
                                        .panel-title {{
                                            color: {};
                                        }}
                                        
                                        /* Dynamic progress bar colors */
                                        .now-playing-progress trough {{
                                            background-color: {} !important;
                                        }}
                                        
                                        .now-playing-progress highlight {{
                                            background: linear-gradient(90deg, {}, {}) !important;
                                        }}
                                        
                                        .now-playing-progress slider {{
                                            background-color: {} !important;
                                            box-shadow: 0 2px 10px {} !important;
                                            border: 3px solid @window_bg_color;
                                        }}
                                        
                                        .now-playing-progress slider:hover {{
                                            box-shadow: 0 3px 16px {} !important;
                                        }}
                                        
                                        /* Dynamic visualizer rings */
                                        .ring-1 {{
                                            border-color: {} !important;
                                        }}
                                        
                                        .ring-2 {{
                                            border-color: {} !important;
                                        }}
                                        
                                        .ring-3 {{
                                            border-color: {} !important;
                                        }}
                                        
                                        .queue-list row:hover {{
                                            background-color: {} !important;
                                        }}
                                        
                                        .lyrics-line-active .lyrics-line {{
                                            color: {} !important;
                                            text-shadow: 0 0 12px {} !important;
                                        }}
                                        
                                        .control-button {{
                                            border-color: {} !important;
                                        }}
                                        
                                        .control-button:hover {{
                                            border-color: {} !important;
                                        }}
                                        
                                        /* Dynamic play button */
                                        .play-button {{
                                            background: linear-gradient(135deg, {}, {}) !important;
                                            box-shadow: 0 6px 24px {} !important;
                                            color: {} !important;
                                        }}
                                        
                                        .play-button:hover {{
                                            background: linear-gradient(135deg, {}, {}) !important;
                                            box-shadow: 0 8px 32px {} !important;
                                        }}",
                                        accent_soft.to_css_rgba(0.65),
                                        accent.to_css_rgba(0.2),
                                        accent_deep.to_css_rgba(0.12),
                                        accent.to_css_rgba(0.12),
                                        accent.to_css_rgba(0.25),
                                        accent_deep.to_css_rgba(0.35),
                                        accent.to_css_rgba(0.35),
                                        accent.to_css_rgba(0.2),
                                        accent_glow.to_css_rgba(0.35),
                                        title_color,
                                        subtitle_color,
                                        panel_text,
                                        // Progress bar
                                        accent.to_css_rgba(0.16),  // trough
                                        accent.to_css_rgba(1.0),   // highlight start
                                        accent_soft.to_css_rgba(0.9),   // highlight end
                                        accent.to_css_rgba(1.0),   // slider
                                        accent.to_css_rgba(0.5),   // slider shadow
                                        accent.to_css_rgba(0.6),   // slider hover shadow
                                        // Rings
                                        accent.to_css_rgba(0.2),  // ring-1
                                        accent.to_css_rgba(0.32),  // ring-2
                                        accent.to_css_rgba(0.45),  // ring-3
                                        // Queue hover
                                        accent.to_css_rgba(0.08),
                                        // Lyrics active
                                        accent.to_css_rgba(0.95),
                                        accent.to_css_rgba(0.4),
                                        // Controls
                                        accent.to_css_rgba(0.25),
                                        accent.to_css_rgba(0.4),
                                        // Play button
                                        accent.to_css_rgba(1.0),   // background start
                                        accent_soft.to_css_rgba(0.9),  // background end
                                        accent.to_css_rgba(0.4),   // shadow
                                        accent.get_text_color(),   // text color
                                        accent_soft.to_css_rgba(0.95),  // hover background start
                                        accent.to_css_rgba(1.0),   // hover background end
                                        accent.to_css_rgba(0.5)    // hover shadow
                                    );
                                    
                                    let provider = gtk4::CssProvider::new();
                                    provider.load_from_data(&css);
                                    
                                    bg.style_context().add_provider(
                                        &provider,
                                        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                                    );
                                    
                                    // Apply to progress bar
                                    if let Some(ref prog) = *progress.borrow() {
                                        prog.style_context().add_provider(
                                            &provider,
                                            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                                        );
                                    }
                                    
                                    // Apply to play button
                                    if let Some(ref btn) = *play_button.borrow() {
                                        btn.style_context().add_provider(
                                            &provider,
                                            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                                        );
                                    }
                                    
                                    // Apply to prev button
                                    if let Some(ref btn) = *prev_button.borrow() {
                                        btn.style_context().add_provider(
                                            &provider,
                                            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                                        );
                                    }
                                    
                                    // Apply to next button
                                    if let Some(ref btn) = *next_button.borrow() {
                                        btn.style_context().add_provider(
                                            &provider,
                                            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                                        );
                                    }
                                    
                                    // Apply to visualizer rings
                                    if let Some(ref ring) = *ring1.borrow() {
                                        ring.style_context().add_provider(
                                            &provider,
                                            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                                        );
                                    }
                                    if let Some(ref ring) = *ring2.borrow() {
                                        ring.style_context().add_provider(
                                            &provider,
                                            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                                        );
                                    }
                                    if let Some(ref ring) = *ring3.borrow() {
                                        ring.style_context().add_provider(
                                            &provider,
                                            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                                        );
                                    }
                                }
                            }
                        }),
                    );
                }
            }
        }

        // Update mini cover
        if let Some(ref cover) = *self.mini_cover.borrow() {
            cover.set_from_file(Some(DEFAULT_COVER_PATH));
            if let Some(url) = chosen_thumbnail {
                if url.starts_with("http://") || url.starts_with("https://") {
                    load_image_async(
                        cover.clone(),
                        url.to_string(),
                        Some((self.cover_token.clone(), cover_token)),
                    );
                }
            }
        }
    }

    pub fn show_loading(&self, message: &str) {
        if let Some(ref title) = *self.title.borrow() {
            title.set_text("Loading...");
        }
        if let Some(ref artist) = *self.artist.borrow() {
            artist.set_text(message);
        }
        if let Some(ref progress) = *self.progress.borrow() {
            progress.set_sensitive(false);
            progress.set_value(0.0);
        }
        
        // Disable buttons during loading
        if let Some(ref prev) = *self.prev_button.borrow() {
            prev.set_sensitive(false);
        }
        if let Some(ref next) = *self.next_button.borrow() {
            next.set_sensitive(false);
        }
        
        // Update mini player
        if let Some(ref title) = *self.mini_title.borrow() {
            title.set_text("Loading...");
        }
        if let Some(ref artist) = *self.mini_artist.borrow() {
            artist.set_text(message);
        }
    }
    
    fn enable_controls(&self) {
        if let Some(ref prev) = *self.prev_button.borrow() {
            prev.set_sensitive(true);
        }
        if let Some(ref next) = *self.next_button.borrow() {
            next.set_sensitive(true);
        }
    }

    pub fn show_error(&self, error: &str) {
        self.enable_controls();
        
        if let Some(ref title) = *self.title.borrow() {
            title.set_text("Not available");
        }
        if let Some(ref artist) = *self.artist.borrow() {
            artist.set_text(error);
        }
        if let Some(ref progress) = *self.progress.borrow() {
            progress.set_sensitive(false);
            progress.set_value(0.0);
        }
        if let Some(ref play_button) = *self.play_button.borrow() {
            play_button.set_icon_name("media-playback-start-symbolic");
        }
        if let Some(ref cover) = *self.cover.borrow() {
            cover.set_from_file(Some(DEFAULT_COVER_PATH));
        }

        // Update mini player for error state
        if let Some(ref title) = *self.mini_title.borrow() {
            title.set_text("Not available");
        }
        if let Some(ref artist) = *self.mini_artist.borrow() {
            artist.set_text(error);
        }

        self.set_lyrics_lines(None, "Lyrics unavailable");
    }

    pub fn set_queue(&self, items: Vec<SearchResult>) {
        *self.queue.borrow_mut() = items;
        self.current_index.set(-1);
        self.queue_rows.borrow_mut().clear();

        if let Some(ref queue_list) = *self.queue_list.borrow() {
            clear_listbox(queue_list);
            for item in self.queue.borrow().iter() {
                let row = song_card_row(
                    &item.title,
                    &item.artist,
                    &item.duration,
                    item.thumbnail_url.as_deref(),
                );
                queue_list.append(&row);
                self.queue_rows.borrow_mut().push(row);
            }
        }
    }

    pub fn set_current_index(&self, index: usize) {
        let prev_index = self.current_index.get();
        self.current_index.set(index as i32);
        self.update_queue_highlight(prev_index, index as i32);
    }

    fn update_queue_highlight(&self, prev_index: i32, new_index: i32) {
        let rows = self.queue_rows.borrow();

        // Remove highlight from previous
        if prev_index >= 0 {
            if let Some(row) = rows.get(prev_index as usize) {
                row.remove_css_class("queue-item-playing");
            }
        }

        // Add highlight to current
        if new_index >= 0 {
            if let Some(row) = rows.get(new_index as usize) {
                row.add_css_class("queue-item-playing");

                // Scroll to the current item
                if let Some(ref scroller) = *self.queue_scroller.borrow() {
                    let vadj = scroller.vadjustment();
                    let allocation = row.allocation();
                    let row_y = allocation.y() as f64;
                    let row_height = allocation.height() as f64;
                    let page_size = vadj.page_size();

                    // Center the row in the viewport
                    let target_scroll = (row_y + row_height / 2.0 - page_size / 2.0)
                        .max(0.0)
                        .min(vadj.upper() - page_size);

                    // Smooth scroll
                    let current_scroll = vadj.value();
                    let steps = 8;
                    let step_size = (target_scroll - current_scroll) / steps as f64;

                    for i in 1..=steps {
                        let scroller_clone = scroller.clone();
                        let final_value = current_scroll + step_size * i as f64;
                        glib::timeout_add_local(Duration::from_millis(i * 15), move || {
                            scroller_clone.vadjustment().set_value(final_value);
                            ControlFlow::Break
                        });
                    }
                }
            }
        }
    }

    pub fn current_item(&self) -> Option<SearchResult> {
        let index = self.current_index.get();
        if index < 0 {
            return None;
        }
        self.queue.borrow().get(index as usize).cloned()
    }

    pub fn shift_index(&self, delta: i32) -> Option<SearchResult> {
        let queue = self.queue.borrow();
        if queue.is_empty() {
            return None;
        }

        let prev_index = self.current_index.get();
        let mut index = prev_index;
        if index < 0 {
            index = 0;
        } else {
            index = (index + delta).clamp(0, (queue.len() - 1) as i32);
        }
        self.current_index.set(index);
        drop(queue);

        self.update_queue_highlight(prev_index, index);

        self.queue.borrow().get(index as usize).cloned()
    }

    pub fn toggle_play_pause(&self) {
        let binding = self.media.borrow();
        let Some(media) = binding.as_ref() else {
            return;
        };

        if media.is_playing() {
            media.pause();
            self.stop_visualizer();
            // Update both play buttons to play icon
            if let Some(ref play_button) = *self.play_button.borrow() {
                play_button.set_icon_name("media-playback-start-symbolic");
            }
            if let Some(ref play_button) = *self.mini_play_button.borrow() {
                play_button.set_icon_name("media-playback-start-symbolic");
            }
        } else {
            media.play();
            self.start_visualizer();
            // Update both play buttons to pause icon
            if let Some(ref play_button) = *self.play_button.borrow() {
                play_button.set_icon_name("media-playback-pause-symbolic");
            }
            if let Some(ref play_button) = *self.mini_play_button.borrow() {
                play_button.set_icon_name("media-playback-pause-symbolic");
            }
        }
    }

    pub fn queue_list(&self) -> Option<gtk4::ListBox> {
        self.queue_list.borrow().clone()
    }

    pub fn play_button(&self) -> Option<gtk4::Button> {
        self.play_button.borrow().clone()
    }

    pub fn prev_button(&self) -> Option<gtk4::Button> {
        self.prev_button.borrow().clone()
    }

    pub fn next_button(&self) -> Option<gtk4::Button> {
        self.next_button.borrow().clone()
    }
}

fn update_lyrics_position(
    seconds: f64,
    lines: &Rc<RefCell<Vec<LyricLine>>>,
    rows: &Rc<RefCell<Vec<gtk4::ListBoxRow>>>,
    scroller: &Rc<RefCell<Option<gtk4::ScrolledWindow>>>,
    current_index: &Rc<Cell<i32>>,
) {
    let lines_ref = lines.borrow();
    if lines_ref.is_empty() {
        return;
    }

    let mut next_index = -1;
    for (idx, line) in lines_ref.iter().enumerate() {
        if line.timestamp <= seconds {
            next_index = idx as i32;
        } else {
            break;
        }
    }

    let prev_index = current_index.get();
    if next_index == prev_index {
        return;
    }
    current_index.set(next_index);
    drop(lines_ref);

    let rows_ref = rows.borrow();
    if prev_index >= 0 {
        if let Some(row) = rows_ref.get(prev_index as usize) {
            row.remove_css_class("lyrics-line-active");
        }
    }
    if next_index >= 0 {
        if let Some(row) = rows_ref.get(next_index as usize) {
            row.add_css_class("lyrics-line-active");
            
            // Auto-scroll to active line
            if let Some(ref scroll) = *scroller.borrow() {
                let vadj = scroll.vadjustment();
                // Get row allocation
                let allocation = row.allocation();
                let row_y = allocation.y() as f64;
                let row_height = allocation.height() as f64;
                
                // Get visible area
                let page_size = vadj.page_size();
                let current_scroll = vadj.value();
                
                // Center the active line in the viewport
                let target_scroll = (row_y + row_height / 2.0 - page_size / 2.0)
                    .max(0.0)
                    .min(vadj.upper() - page_size);
                
                // Smooth scroll with animation
                let steps = 10;
                let step_size = (target_scroll - current_scroll) / steps as f64;
                
                for i in 1..=steps {
                    let scroll_clone = scroll.clone();
                    let final_value = current_scroll + step_size * i as f64;
                    glib::timeout_add_local(Duration::from_millis(i * 20), move || {
                        let vadj = scroll_clone.vadjustment();
                        vadj.set_value(final_value);
                        ControlFlow::Break
                    });
                }
            }
        }
    }
}

fn build_lyrics_row(text: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);

    let label = gtk4::Label::new(Some(text));
    label.set_wrap(true);
    label.set_justify(gtk4::Justification::Center);
    label.add_css_class("lyrics-line");

    row.set_child(Some(&label));
    row
}

fn build_lyrics_placeholder_row(text: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);

    let label = gtk4::Label::new(Some(text));
    label.set_wrap(true);
    label.set_justify(gtk4::Justification::Center);
    label.add_css_class("lyrics-placeholder");

    row.set_child(Some(&label));
    row
}

fn format_duration(seconds: i64) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{}:{:02}", mins, secs)
}
