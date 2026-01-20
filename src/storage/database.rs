use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

/// Song data structure used for liked songs, recent plays, and playlist songs
#[derive(Debug, Clone)]
pub struct Song {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub duration: String,
    pub thumbnail_url: Option<String>,
}

/// Liked song with timestamp
#[derive(Debug, Clone)]
pub struct LikedSong {
    pub song: Song,
    pub liked_at: i64,
}

/// Recent play with timestamp
#[derive(Debug, Clone)]
pub struct RecentPlay {
    pub id: i64,
    pub song: Song,
    pub played_at: i64,
}

/// Playlist metadata
#[derive(Debug, Clone)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
}

/// Song in a playlist with position
#[derive(Debug, Clone)]
pub struct PlaylistSong {
    pub id: i64,
    pub playlist_id: i64,
    pub song: Song,
    pub position: i32,
}

/// Database handle for SQLite operations
#[derive(Clone)]
pub struct Database {
    conn: Rc<RefCell<Connection>>,
}

impl Database {
    /// Initialize database at ~/.local/share/musika/musika.db
    pub fn new() -> Result<Self, String> {
        let db_path = Self::get_db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create database directory: {}", e))?;
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        let db = Self {
            conn: Rc::new(RefCell::new(conn)),
        };

        db.init_tables()?;
        Ok(db)
    }

    fn get_db_path() -> Result<PathBuf, String> {
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".local/share")
            });

        Ok(data_dir.join("musika").join("musika.db"))
    }

    fn init_tables(&self) -> Result<(), String> {
        let conn = self.conn.borrow();

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS liked_songs (
                video_id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                duration TEXT NOT NULL,
                thumbnail_url TEXT,
                liked_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS recent_plays (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                video_id TEXT NOT NULL,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                duration TEXT NOT NULL,
                thumbnail_url TEXT,
                played_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS playlists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS playlist_songs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                playlist_id INTEGER NOT NULL,
                video_id TEXT NOT NULL,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                duration TEXT NOT NULL,
                thumbnail_url TEXT,
                position INTEGER NOT NULL,
                FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_recent_plays_date ON recent_plays(played_at DESC);
            CREATE INDEX IF NOT EXISTS idx_playlist_songs_playlist ON playlist_songs(playlist_id, position);
            "
        ).map_err(|e| format!("Failed to create tables: {}", e))?;

        Ok(())
    }

    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    // ==================== Liked Songs ====================

    /// Add a song to liked songs
    pub fn like_song(&self, song: &Song) -> Result<(), String> {
        let conn = self.conn.borrow();
        conn.execute(
            "INSERT OR REPLACE INTO liked_songs (video_id, title, artist, duration, thumbnail_url, liked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                song.video_id,
                song.title,
                song.artist,
                song.duration,
                song.thumbnail_url,
                Self::current_timestamp()
            ],
        ).map_err(|e| format!("Failed to like song: {}", e))?;
        Ok(())
    }

    /// Remove a song from liked songs
    pub fn unlike_song(&self, video_id: &str) -> Result<(), String> {
        let conn = self.conn.borrow();
        conn.execute(
            "DELETE FROM liked_songs WHERE video_id = ?1",
            params![video_id],
        ).map_err(|e| format!("Failed to unlike song: {}", e))?;
        Ok(())
    }

    /// Check if a song is liked
    pub fn is_song_liked(&self, video_id: &str) -> bool {
        let conn = self.conn.borrow();
        conn.query_row(
            "SELECT 1 FROM liked_songs WHERE video_id = ?1",
            params![video_id],
            |_| Ok(()),
        ).is_ok()
    }

    /// Get all liked songs ordered by liked_at descending
    pub fn get_liked_songs(&self) -> Result<Vec<LikedSong>, String> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT video_id, title, artist, duration, thumbnail_url, liked_at
             FROM liked_songs ORDER BY liked_at DESC"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;

        let songs = stmt.query_map([], |row| {
            Ok(LikedSong {
                song: Song {
                    video_id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    duration: row.get(3)?,
                    thumbnail_url: row.get(4)?,
                },
                liked_at: row.get(5)?,
            })
        }).map_err(|e| format!("Failed to query liked songs: {}", e))?;

        songs.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect liked songs: {}", e))
    }

    /// Get count of liked songs
    pub fn get_liked_songs_count(&self) -> i64 {
        let conn = self.conn.borrow();
        conn.query_row(
            "SELECT COUNT(*) FROM liked_songs",
            [],
            |row| row.get(0),
        ).unwrap_or(0)
    }

    // ==================== Recent Plays ====================

    /// Add a song to recent plays
    pub fn add_recent_play(&self, song: &Song) -> Result<(), String> {
        let conn = self.conn.borrow();
        // Ensure one entry per song in recent plays; keep the latest.
        conn.execute(
            "DELETE FROM recent_plays WHERE video_id = ?1",
            params![song.video_id],
        ).map_err(|e| format!("Failed to remove existing recent play: {}", e))?;
        conn.execute(
            "INSERT INTO recent_plays (video_id, title, artist, duration, thumbnail_url, played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                song.video_id,
                song.title,
                song.artist,
                song.duration,
                song.thumbnail_url,
                Self::current_timestamp()
            ],
        ).map_err(|e| format!("Failed to add recent play: {}", e))?;

        // Keep only the last 50 entries
        conn.execute(
            "DELETE FROM recent_plays WHERE id NOT IN (
                SELECT id FROM recent_plays ORDER BY played_at DESC LIMIT 50
            )",
            [],
        ).map_err(|e| format!("Failed to clean recent plays: {}", e))?;

        Ok(())
    }

    /// Get recent plays (last 50)
    pub fn get_recent_plays(&self) -> Result<Vec<RecentPlay>, String> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT id, video_id, title, artist, duration, thumbnail_url, played_at
             FROM recent_plays
             WHERE id IN (
                 SELECT id FROM recent_plays rp
                 WHERE rp.video_id = recent_plays.video_id
                 ORDER BY rp.played_at DESC LIMIT 1
             )
             ORDER BY played_at DESC LIMIT 50"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;

        let plays = stmt.query_map([], |row| {
            Ok(RecentPlay {
                id: row.get(0)?,
                song: Song {
                    video_id: row.get(1)?,
                    title: row.get(2)?,
                    artist: row.get(3)?,
                    duration: row.get(4)?,
                    thumbnail_url: row.get(5)?,
                },
                played_at: row.get(6)?,
            })
        }).map_err(|e| format!("Failed to query recent plays: {}", e))?;

        plays.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect recent plays: {}", e))
    }

    /// Clear all recent plays
    pub fn clear_recent_plays(&self) -> Result<(), String> {
        let conn = self.conn.borrow();
        conn.execute("DELETE FROM recent_plays", [])
            .map_err(|e| format!("Failed to clear recent plays: {}", e))?;
        Ok(())
    }

    // ==================== Playlists ====================

    /// Create a new playlist
    pub fn create_playlist(&self, name: &str) -> Result<Playlist, String> {
        let conn = self.conn.borrow();
        let created_at = Self::current_timestamp();

        conn.execute(
            "INSERT INTO playlists (name, created_at) VALUES (?1, ?2)",
            params![name, created_at],
        ).map_err(|e| format!("Failed to create playlist: {}", e))?;

        let id = conn.last_insert_rowid();
        Ok(Playlist {
            id,
            name: name.to_string(),
            created_at,
        })
    }

    /// Rename a playlist
    pub fn rename_playlist(&self, playlist_id: i64, new_name: &str) -> Result<(), String> {
        let conn = self.conn.borrow();
        conn.execute(
            "UPDATE playlists SET name = ?1 WHERE id = ?2",
            params![new_name, playlist_id],
        ).map_err(|e| format!("Failed to rename playlist: {}", e))?;
        Ok(())
    }

    /// Delete a playlist
    pub fn delete_playlist(&self, playlist_id: i64) -> Result<(), String> {
        let conn = self.conn.borrow();
        // Delete songs first (foreign key constraint)
        conn.execute(
            "DELETE FROM playlist_songs WHERE playlist_id = ?1",
            params![playlist_id],
        ).map_err(|e| format!("Failed to delete playlist songs: {}", e))?;

        conn.execute(
            "DELETE FROM playlists WHERE id = ?1",
            params![playlist_id],
        ).map_err(|e| format!("Failed to delete playlist: {}", e))?;
        Ok(())
    }

    /// Get all playlists
    pub fn get_playlists(&self) -> Result<Vec<Playlist>, String> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT id, name, created_at FROM playlists ORDER BY created_at DESC"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;

        let playlists = stmt.query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        }).map_err(|e| format!("Failed to query playlists: {}", e))?;

        playlists.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect playlists: {}", e))
    }

    /// Get playlist by ID
    pub fn get_playlist(&self, playlist_id: i64) -> Result<Option<Playlist>, String> {
        let conn = self.conn.borrow();
        let result = conn.query_row(
            "SELECT id, name, created_at FROM playlists WHERE id = ?1",
            params![playlist_id],
            |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                })
            },
        );

        match result {
            Ok(playlist) => Ok(Some(playlist)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Failed to get playlist: {}", e)),
        }
    }

    // ==================== Playlist Songs ====================

    /// Add a song to a playlist
    pub fn add_song_to_playlist(&self, playlist_id: i64, song: &Song) -> Result<(), String> {
        if self.is_song_in_playlist(playlist_id, &song.video_id) {
            return Ok(());
        }
        let conn = self.conn.borrow();

        // Get the next position
        let position: i32 = conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_songs WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        ).unwrap_or(0);

        conn.execute(
            "INSERT INTO playlist_songs (playlist_id, video_id, title, artist, duration, thumbnail_url, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                playlist_id,
                song.video_id,
                song.title,
                song.artist,
                song.duration,
                song.thumbnail_url,
                position
            ],
        ).map_err(|e| format!("Failed to add song to playlist: {}", e))?;
        Ok(())
    }

    /// Remove a song from a playlist
    pub fn remove_song_from_playlist(&self, playlist_id: i64, song_id: i64) -> Result<(), String> {
        let conn = self.conn.borrow();
        conn.execute(
            "DELETE FROM playlist_songs WHERE id = ?1 AND playlist_id = ?2",
            params![song_id, playlist_id],
        ).map_err(|e| format!("Failed to remove song from playlist: {}", e))?;
        Ok(())
    }

    /// Swap positions of two songs within a playlist
    pub fn swap_playlist_song_positions(
        &self,
        playlist_id: i64,
        song_id: i64,
        other_song_id: i64,
    ) -> Result<(), String> {
        let mut conn = self.conn.borrow_mut();
        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to start transaction: {}", e))?;

        let position: i32 = tx
            .query_row(
                "SELECT position FROM playlist_songs WHERE id = ?1 AND playlist_id = ?2",
                params![song_id, playlist_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to get song position: {}", e))?;

        let other_position: i32 = tx
            .query_row(
                "SELECT position FROM playlist_songs WHERE id = ?1 AND playlist_id = ?2",
                params![other_song_id, playlist_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to get other song position: {}", e))?;

        tx.execute(
            "UPDATE playlist_songs SET position = ?1 WHERE id = ?2 AND playlist_id = ?3",
            params![other_position, song_id, playlist_id],
        )
        .map_err(|e| format!("Failed to update song position: {}", e))?;

        tx.execute(
            "UPDATE playlist_songs SET position = ?1 WHERE id = ?2 AND playlist_id = ?3",
            params![position, other_song_id, playlist_id],
        )
        .map_err(|e| format!("Failed to update other song position: {}", e))?;

        tx.commit()
            .map_err(|e| format!("Failed to commit song reorder: {}", e))?;

        Ok(())
    }

    /// Get songs in a playlist
    pub fn get_playlist_songs(&self, playlist_id: i64) -> Result<Vec<PlaylistSong>, String> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT id, playlist_id, video_id, title, artist, duration, thumbnail_url, position
             FROM playlist_songs WHERE playlist_id = ?1 ORDER BY position ASC"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;

        let songs = stmt.query_map(params![playlist_id], |row| {
            Ok(PlaylistSong {
                id: row.get(0)?,
                playlist_id: row.get(1)?,
                song: Song {
                    video_id: row.get(2)?,
                    title: row.get(3)?,
                    artist: row.get(4)?,
                    duration: row.get(5)?,
                    thumbnail_url: row.get(6)?,
                },
                position: row.get(7)?,
            })
        }).map_err(|e| format!("Failed to query playlist songs: {}", e))?;

        songs.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect playlist songs: {}", e))
    }

    /// Get count of songs in a playlist
    pub fn get_playlist_song_count(&self, playlist_id: i64) -> i64 {
        let conn = self.conn.borrow();
        conn.query_row(
            "SELECT COUNT(*) FROM playlist_songs WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        ).unwrap_or(0)
    }

    /// Check if a song is in a playlist
    pub fn is_song_in_playlist(&self, playlist_id: i64, video_id: &str) -> bool {
        let conn = self.conn.borrow();
        conn.query_row(
            "SELECT 1 FROM playlist_songs WHERE playlist_id = ?1 AND video_id = ?2",
            params![playlist_id, video_id],
            |_| Ok(()),
        ).is_ok()
    }
}
