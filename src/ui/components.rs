use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::sync::mpsc;
use std::time::Duration;

use glib::ControlFlow;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::config::{
    COVER_SIZE_MINI, DEFAULT_COVER_PATH, ICON_MUSIC, ICON_PLAYLIST, MARGIN_TINY, POLL_INTERVAL_MS,
};

const IMAGE_CACHE_LIMIT: usize = 200;
const IMAGE_CACHE_PREFIX: &str = "cloudmusic-image-cache";
const IMAGE_CACHE_MAX_FILES: usize = 500;

struct ImageCache {
    map: HashMap<String, Vec<u8>>,
    order: VecDeque<String>,
}

impl ImageCache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        if self.map.contains_key(key) {
            self.touch(key);
            return self.map.get(key).cloned();
        }
        None
    }

    fn insert(&mut self, key: String, payload: Vec<u8>) {
        if self.map.contains_key(&key) {
            self.touch(&key);
            self.map.insert(key, payload);
            return;
        }

        self.map.insert(key.clone(), payload);
        self.order.push_back(key);
        while self.order.len() > IMAGE_CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
    }

    fn touch(&mut self, key: &str) {
        if let Some(pos) = self.order.iter().position(|entry| entry == key) {
            self.order.remove(pos);
        }
        self.order.push_back(key.to_string());
    }
}

static IMAGE_CACHE: OnceLock<Mutex<ImageCache>> = OnceLock::new();
static IMAGE_CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
static IMAGE_CACHE_FILE_COUNT: AtomicUsize = AtomicUsize::new(0);

fn image_cache_get(url: &str) -> Option<Vec<u8>> {
    let cache = IMAGE_CACHE.get_or_init(|| Mutex::new(ImageCache::new()));
    cache.lock().ok().and_then(|mut cache| cache.get(url))
}

fn image_cache_put(url: &str, payload: Vec<u8>) {
    let cache = IMAGE_CACHE.get_or_init(|| Mutex::new(ImageCache::new()));
    if let Ok(mut cache) = cache.lock() {
        cache.insert(url.to_string(), payload);
    }
}

fn image_cache_dir() -> PathBuf {
    IMAGE_CACHE_DIR
        .get_or_init(|| std::env::temp_dir().join(IMAGE_CACHE_PREFIX))
        .clone()
}

fn image_cache_key(url: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn image_cache_path(url: &str) -> PathBuf {
    image_cache_dir().join(image_cache_key(url))
}

fn image_cache_read_disk(url: &str) -> Option<Vec<u8>> {
    std::fs::read(image_cache_path(url)).ok()
}

fn image_cache_write_disk(url: &str, payload: &[u8]) {
    if IMAGE_CACHE_FILE_COUNT.load(Ordering::Relaxed) >= IMAGE_CACHE_MAX_FILES {
        return;
    }
    let dir = image_cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = image_cache_path(url);
    if std::fs::write(path, payload).is_ok() {
        IMAGE_CACHE_FILE_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

/// Creates a song card row for list boxes
pub fn song_card_row(
    title: &str,
    artist: &str,
    duration: &str,
    thumbnail_url: Option<&str>,
) -> gtk4::ListBoxRow {
    song_card_row_with_like(title, artist, duration, thumbnail_url, None)
}

/// Creates a song card row with optional like button
pub fn song_card_row_with_like(
    title: &str,
    artist: &str,
    duration: &str,
    thumbnail_url: Option<&str>,
    like_button: Option<gtk4::Button>,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let action = adw::ActionRow::new();
    let duration_text = if duration.trim().is_empty() {
        "--:--"
    } else {
        duration
    };
    let duration_label = gtk4::Label::new(Some(duration_text));
    duration_label.add_css_class("dim-label");

    action.set_title(title);
    action.set_subtitle(artist);
    action.add_prefix(&cover_widget(thumbnail_url, COVER_SIZE_MINI));

    if let Some(btn) = like_button {
        action.add_suffix(&btn);
    }
    action.add_suffix(&duration_label);
    action.set_activatable(true);
    action.add_css_class("song-card");

    row.set_child(Some(&action));
    row
}

/// Creates a cover image widget with optional async loading
pub fn cover_widget(thumbnail_url: Option<&str>, size: i32) -> gtk4::Widget {
    let image = gtk4::Image::from_icon_name(ICON_MUSIC);
    image.set_pixel_size(size);
    image.set_size_request(size, size);
    image.set_halign(gtk4::Align::Center);
    image.set_valign(gtk4::Align::Center);
    image.set_overflow(gtk4::Overflow::Hidden);
    image.add_css_class("album-cover-image");

    let frame = gtk4::Frame::new(None);
    frame.set_size_request(size, size);
    frame.set_overflow(gtk4::Overflow::Hidden);
    frame.add_css_class("album-cover");
    frame.add_css_class("album-cover-frame");

    if size <= COVER_SIZE_MINI {
        frame.add_css_class("album-cover-small");
    }

    if let Some(url) = thumbnail_url {
        if url.starts_with("http://") || url.starts_with("https://") {
            load_image_async(image.clone(), url.to_string(), None);
        }
    }

    frame.set_child(Some(&image));
    frame.upcast()
}

/// RGB color representation
#[derive(Clone, Copy, Debug)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    /// Convert to CSS rgba string with alpha
    pub fn to_css_rgba(&self, alpha: f32) -> String {
        format!("rgba({}, {}, {}, {})", self.r, self.g, self.b, alpha)
    }
    
    /// Calculate luminance to determine if color is light or dark
    pub fn is_light(&self) -> bool {
        // Using relative luminance formula
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;
        
        let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        luminance > 0.5
    }
    
    /// Get appropriate text color (white for dark backgrounds, black for light)
    pub fn get_text_color(&self) -> &str {
        if self.is_light() {
            "rgba(0, 0, 0, 0.87)"
        } else {
            "rgba(255, 255, 255, 0.95)"
        }
    }

    /// Get text color with a configurable alpha channel
    pub fn text_rgba(&self, alpha: f32) -> String {
        let clamped = alpha.clamp(0.0, 1.0);
        if self.is_light() {
            format!("rgba(0, 0, 0, {})", clamped)
        } else {
            format!("rgba(255, 255, 255, {})", clamped)
        }
    }

    /// Blend this color with another color
    pub fn mix(&self, other: RgbColor, amount: f32) -> RgbColor {
        let amt = amount.clamp(0.0, 1.0);
        let inv = 1.0 - amt;
        let r = (self.r as f32 * inv + other.r as f32 * amt).round().clamp(0.0, 255.0) as u8;
        let g = (self.g as f32 * inv + other.g as f32 * amt).round().clamp(0.0, 255.0) as u8;
        let b = (self.b as f32 * inv + other.b as f32 * amt).round().clamp(0.0, 255.0) as u8;

        RgbColor { r, g, b }
    }

    /// Boost or reduce saturation by scaling distance from the average channel
    pub fn with_saturation(&self, factor: f32) -> RgbColor {
        let avg = (self.r as f32 + self.g as f32 + self.b as f32) / 3.0;
        let r = (avg + (self.r as f32 - avg) * factor).round().clamp(0.0, 255.0) as u8;
        let g = (avg + (self.g as f32 - avg) * factor).round().clamp(0.0, 255.0) as u8;
        let b = (avg + (self.b as f32 - avg) * factor).round().clamp(0.0, 255.0) as u8;

        RgbColor { r, g, b }
    }
}

/// Extracts the dominant color from a pixbuf by sampling pixels
pub fn extract_dominant_color(pixbuf: &gtk4::gdk_pixbuf::Pixbuf) -> RgbColor {
    let width = pixbuf.width() as usize;
    let height = pixbuf.height() as usize;
    let rowstride = pixbuf.rowstride() as usize;
    let n_channels = pixbuf.n_channels() as usize;
    let pixels = unsafe { pixbuf.pixels() };

    // Sample pixels from the image (skip edges, sample every nth pixel for performance)
    let sample_step = ((width.min(height)) / 20).max(1);
    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut count: u64 = 0;
    let mut best_color = RgbColor { r: 0, g: 0, b: 0 };
    let mut best_score = 0.0f32;

    // Focus on center region for better color extraction
    let start_x = width / 4;
    let end_x = width * 3 / 4;
    let start_y = height / 4;
    let end_y = height * 3 / 4;

    for y in (start_y..end_y).step_by(sample_step) {
        for x in (start_x..end_x).step_by(sample_step) {
            let offset = y * rowstride + x * n_channels;
            if offset + 2 < pixels.len() {
                let r = pixels[offset] as u64;
                let g = pixels[offset + 1] as u64;
                let b = pixels[offset + 2] as u64;

                // Skip very dark or very light pixels
                let brightness = (r + g + b) / 3;
                if brightness > 30 && brightness < 220 {
                    r_sum += r;
                    g_sum += g;
                    b_sum += b;
                    count += 1;

                    // Track a more saturated candidate to avoid washed-out averages
                    let max = r.max(g).max(b) as f32;
                    let min = r.min(g).min(b) as f32;
                    let saturation = if max > 0.0 { (max - min) / max } else { 0.0 };
                    let brightness_norm = brightness as f32 / 255.0;
                    let score = saturation * (1.0 - (brightness_norm - 0.5).abs());
                    if score > best_score {
                        best_score = score;
                        best_color = RgbColor {
                            r: r as u8,
                            g: g as u8,
                            b: b as u8,
                        };
                    }
                }
            }
        }
    }

    if count == 0 {
        // Fallback to a default color if no suitable pixels found
        return RgbColor { r: 100, g: 100, b: 140 };
    }

    let avg = RgbColor {
        r: (r_sum / count) as u8,
        g: (g_sum / count) as u8,
        b: (b_sum / count) as u8,
    };

    if best_score > 0.0 {
        avg.mix(best_color, 0.35)
    } else {
        avg
    }
}

/// Loads an image asynchronously from a URL
pub fn load_image_async(
    image: gtk4::Image,
    url: String,
    token_guard: Option<(Rc<Cell<u64>>, u64)>,
) {
    load_image_async_with_callback(image, url, token_guard, None::<fn(RgbColor)>);
}

/// Loads an image asynchronously from a URL with optional color callback
pub fn load_image_async_with_callback<F>(
    image: gtk4::Image,
    url: String,
    token_guard: Option<(Rc<Cell<u64>>, u64)>,
    on_color_extracted: Option<F>,
)
where
    F: Fn(RgbColor) + 'static,
{
    let (sender, receiver) = mpsc::channel::<Vec<u8>>();
    if let Some(payload) = image_cache_get(&url) {
        let _ = sender.send(payload);
    } else {
        std::thread::spawn(move || {
            if let Some(payload) = image_cache_read_disk(&url) {
                image_cache_put(&url, payload.clone());
                let _ = sender.send(payload);
                return;
            }
            let bytes = match reqwest::blocking::get(&url).and_then(|resp| resp.bytes()) {
                Ok(bytes) => bytes,
                Err(_) => return,
            };
            let payload = bytes.to_vec();
            image_cache_put(&url, payload.clone());
            image_cache_write_disk(&url, &payload);
            let _ = sender.send(payload);
        });
    }

    let receiver = Rc::new(RefCell::new(receiver));
    let on_color_extracted = on_color_extracted.map(|f| Rc::new(RefCell::new(Some(f))));

    glib::timeout_add_local(Duration::from_millis(POLL_INTERVAL_MS), move || {
        if let Some((guard, token)) = token_guard.as_ref() {
            if guard.get() != *token {
                return ControlFlow::Break;
            }
        }

        match receiver.borrow().try_recv() {
            Ok(payload) => {
                let loader = gtk4::gdk_pixbuf::PixbufLoader::new();
                if loader.write(&payload).is_err() || loader.close().is_err() {
                    return ControlFlow::Break;
                }
                if let Some(pixbuf) = loader.pixbuf() {
                    // Extract color before setting the image
                    if let Some(ref callback_cell) = on_color_extracted {
                        if let Some(callback) = callback_cell.borrow_mut().take() {
                            let color = extract_dominant_color(&pixbuf);
                            callback(color);
                        }
                    }

                    let texture = gtk4::gdk::Texture::for_pixbuf(&pixbuf);
                    image.set_paintable(Some(&texture));
                }
                ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => ControlFlow::Break,
        }
    });
}

/// Creates a placeholder row for empty states
pub fn placeholder_row(text: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);

    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    container.set_margin_top(32);
    container.set_margin_bottom(32);
    container.set_halign(gtk4::Align::Center);

    let icon = gtk4::Image::from_icon_name(ICON_MUSIC);
    icon.set_pixel_size(48);
    icon.add_css_class("dim-label");

    let label = gtk4::Label::new(Some(text));
    label.add_css_class("dim-label");

    container.append(&icon);
    container.append(&label);

    row.set_child(Some(&container));
    row
}

/// Creates a section with a title and content
pub fn section(title: &str, icon_name: Option<&str>, child: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, MARGIN_TINY);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, MARGIN_TINY);

    if let Some(icon) = icon_name {
        let icon_widget = gtk4::Image::from_icon_name(icon);
        icon_widget.add_css_class("dim-label");
        header.append(&icon_widget);
    }

    let label = gtk4::Label::new(Some(title));
    label.add_css_class("heading");
    label.set_xalign(0.0);
    header.append(&label);

    container.append(&header);
    container.append(child);
    container
}

/// Creates a playlist row
pub fn playlist_row(name: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, MARGIN_TINY);
    let icon = gtk4::Image::from_icon_name(ICON_PLAYLIST);
    let label = gtk4::Label::new(Some(name));

    label.set_xalign(0.0);
    label.set_hexpand(true);
    container.append(&icon);
    container.append(&label);
    container.set_margin_top(6);
    container.set_margin_bottom(6);
    container.set_margin_start(MARGIN_TINY);
    container.set_margin_end(MARGIN_TINY);

    row.set_child(Some(&container));
    row
}

/// Clears all children from a listbox
pub fn clear_listbox(list: &gtk4::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

/// Creates a loading spinner row
pub fn loading_row(message: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);
    
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    container.set_halign(gtk4::Align::Center);
    container.set_margin_top(24);
    container.set_margin_bottom(24);
    
    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(24, 24);
    
    let label = gtk4::Label::new(Some(message));
    label.add_css_class("dim-label");
    
    container.append(&spinner);
    container.append(&label);
    
    row.set_child(Some(&container));
    row
}

/// Creates a large cover widget for now playing view
pub fn large_cover_widget(size: i32) -> gtk4::Image {
    let image = gtk4::Image::from_file(DEFAULT_COVER_PATH);
    image.set_pixel_size(size);
    image.set_overflow(gtk4::Overflow::Hidden);
    image.add_css_class("album-cover");
    image.add_css_class("album-cover-large");
    image
}
