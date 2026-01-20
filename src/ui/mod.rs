pub mod components;
pub mod header;
pub mod library;
pub mod mini_player;
pub mod now_playing;
pub mod search;
pub mod settings;

pub use header::build_header;
pub use library::build_library_view;
pub use mini_player::build_mini_player;
pub use now_playing::build_now_playing_view;
pub use search::build_search_view;
pub use settings::build_settings_view;
