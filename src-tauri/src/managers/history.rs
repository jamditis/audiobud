use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use log::{debug, error, info, warn};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::AppHandle;
use tauri_specta::Event;

/// Returns true only for a single, plain filename with no path navigation.
/// Recording names are always server-generated (`handy-<timestamp>.wav`), so this
/// rejects any webview-supplied `file_name` that contains a separator, `..`, an
/// absolute/drive/UNC prefix, or an embedded NUL. Combined with the join in
/// `HistoryManager::get_audio_file_path`, a path can never escape `recordings_dir`.
pub(crate) fn is_safe_recording_filename(name: &str) -> bool {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains('\0') {
        return false;
    }
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

/// Database migrations for transcription history.
/// Each migration is applied in order. The library tracks which migrations
/// have been applied using SQLite's user_version pragma.
///
/// Note: For users upgrading from tauri-plugin-sql, migrate_from_tauri_plugin_sql()
/// converts the old _sqlx_migrations table tracking to the user_version pragma,
/// ensuring migrations don't re-run on existing databases.
static MIGRATIONS: &[M] = &[
    M::up(
        "CREATE TABLE IF NOT EXISTS transcription_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_name TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            saved BOOLEAN NOT NULL DEFAULT 0,
            title TEXT NOT NULL,
            transcription_text TEXT NOT NULL
        );",
    ),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_processed_text TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_prompt TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_requested BOOLEAN NOT NULL DEFAULT 0;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN raw_requested BOOLEAN NOT NULL DEFAULT 0;"),
];

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PaginatedHistory {
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(tag = "action")]
pub enum HistoryUpdatePayload {
    #[serde(rename = "added")]
    Added { entry: HistoryEntry },
    #[serde(rename = "updated")]
    Updated { entry: HistoryEntry },
    #[serde(rename = "deleted")]
    Deleted { id: i64 },
    #[serde(rename = "toggled")]
    Toggled { id: i64 },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HistoryEntry {
    pub id: i64,
    pub file_name: String,
    pub timestamp: i64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub post_process_requested: bool,
    /// Whether this dictation was emitted as raw text (the effective raw decision at creation
    /// time, after combining the per-dictation request with the persisted `raw_output` toggle).
    /// Persisted so a retry reproduces the original formatting instead of following whatever raw
    /// mode is active now.
    pub raw_requested: bool,
}

pub struct HistoryManager {
    app_handle: AppHandle,
    recordings_dir: PathBuf,
    db_path: PathBuf,
}

impl HistoryManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // Create recordings directory in app data dir
        let app_data_dir = crate::portable::app_data_dir(app_handle)?;
        let recordings_dir = app_data_dir.join("recordings");
        let db_path = app_data_dir.join("history.db");

        // Ensure recordings directory exists
        if !recordings_dir.exists() {
            fs::create_dir_all(&recordings_dir)?;
            debug!("Created recordings directory: {:?}", recordings_dir);
        }

        let manager = Self {
            app_handle: app_handle.clone(),
            recordings_dir,
            db_path,
        };

        // Initialize database and run migrations synchronously
        manager.init_database()?;

        Ok(manager)
    }

    fn init_database(&self) -> Result<()> {
        info!("Initializing database at {:?}", self.db_path);

        let mut conn = Connection::open(&self.db_path)?;

        // Handle migration from tauri-plugin-sql to rusqlite_migration
        // tauri-plugin-sql used _sqlx_migrations table, rusqlite_migration uses user_version pragma
        self.migrate_from_tauri_plugin_sql(&conn)?;

        // Create migrations object and run to latest version
        let migrations = Migrations::new(MIGRATIONS.to_vec());

        // Validate migrations in debug builds
        #[cfg(debug_assertions)]
        migrations.validate().expect("Invalid migrations");

        // Get current version before migration
        let version_before: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        debug!("Database version before migration: {}", version_before);

        // Apply any pending migrations
        migrations.to_latest(&mut conn)?;

        // Get version after migration
        let version_after: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if version_after > version_before {
            info!(
                "Database migrated from version {} to {}",
                version_before, version_after
            );
        } else {
            debug!("Database already at latest version {}", version_after);
        }

        Ok(())
    }

    /// Migrate from tauri-plugin-sql's migration tracking to rusqlite_migration's.
    /// tauri-plugin-sql used a _sqlx_migrations table, while rusqlite_migration uses
    /// SQLite's user_version pragma. This function checks if the old system was in use
    /// and sets the user_version accordingly so migrations don't re-run.
    fn migrate_from_tauri_plugin_sql(&self, conn: &Connection) -> Result<()> {
        // Check if the old _sqlx_migrations table exists
        let has_sqlx_migrations: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_sqlx_migrations {
            return Ok(());
        }

        // Check current user_version
        let current_version: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if current_version > 0 {
            // Already migrated to rusqlite_migration system
            return Ok(());
        }

        // Get the highest version from the old migrations table
        let old_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if old_version > 0 {
            info!(
                "Migrating from tauri-plugin-sql (version {}) to rusqlite_migration",
                old_version
            );

            // Set user_version to match the old migration state
            conn.pragma_update(None, "user_version", old_version)?;

            // Optionally drop the old migrations table (keeping it doesn't hurt)
            // conn.execute("DROP TABLE IF EXISTS _sqlx_migrations", [])?;

            info!(
                "Migration tracking converted: user_version set to {}",
                old_version
            );
        }

        Ok(())
    }

    fn get_connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    fn map_history_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
        Ok(HistoryEntry {
            id: row.get("id")?,
            file_name: row.get("file_name")?,
            timestamp: row.get("timestamp")?,
            saved: row.get("saved")?,
            title: row.get("title")?,
            transcription_text: row.get("transcription_text")?,
            post_processed_text: row.get("post_processed_text")?,
            post_process_prompt: row.get("post_process_prompt")?,
            post_process_requested: row.get("post_process_requested")?,
            raw_requested: row.get("raw_requested")?,
        })
    }

    pub fn recordings_dir(&self) -> &std::path::Path {
        &self.recordings_dir
    }

    /// Save a new history entry to the database.
    /// The WAV file should already have been written to the recordings directory.
    pub fn save_entry(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        raw_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        let timestamp = Utc::now().timestamp();
        let title = self.format_timestamp_title(timestamp);

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                raw_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &file_name,
                timestamp,
                false,
                &title,
                &transcription_text,
                &post_processed_text,
                &post_process_prompt,
                post_process_requested,
                raw_requested,
            ],
        )?;

        let entry = HistoryEntry {
            id: conn.last_insert_rowid(),
            file_name,
            timestamp,
            saved: false,
            title,
            transcription_text,
            post_processed_text,
            post_process_prompt,
            post_process_requested,
            raw_requested,
        };

        debug!("Saved history entry with id {}", entry.id);

        self.cleanup_old_entries()?;

        // Emit typed event for real-time frontend updates
        if let Err(e) = (HistoryUpdatePayload::Added {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    /// Update an existing history entry with new transcription results (used by retry).
    pub fn update_transcription(
        &self,
        id: i64,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let updated = conn.execute(
            "UPDATE transcription_history
             SET transcription_text = ?1,
                 post_processed_text = ?2,
                 post_process_prompt = ?3
             WHERE id = ?4",
            params![
                transcription_text,
                post_processed_text,
                post_process_prompt,
                id
            ],
        )?;

        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }

        let entry = conn
            .query_row(
                "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested, raw_requested
                 FROM transcription_history WHERE id = ?1",
                params![id],
                Self::map_history_entry,
            )?;

        debug!("Updated transcription for history entry {}", id);

        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    pub fn cleanup_old_entries(&self) -> Result<()> {
        let retention_period = crate::settings::get_recording_retention_period(&self.app_handle);
        let history_limit = crate::settings::get_history_limit(&self.app_handle);
        let conn = self.get_connection()?;
        let deleted_ids = Self::cleanup_old_entries_with_conn(
            &conn,
            &self.recordings_dir,
            retention_period,
            history_limit,
        )?;

        // The lazy cleanup runs inside save_entry, so the History page may be
        // mounted and still showing the trimmed rows (it prepends new entries
        // from the Added event without re-fetching). Emit a Deleted event per
        // trimmed row so the UI drops them instead of offering playback and
        // retry on entries that no longer exist.
        for id in deleted_ids {
            if let Err(e) = (HistoryUpdatePayload::Deleted { id }).emit(&self.app_handle) {
                error!("Failed to emit history-updated event: {}", e);
            }
        }

        Ok(())
    }

    /// Core of `cleanup_old_entries`, extracted with an explicit connection +
    /// recordings dir (same pattern as `delete_entry_with_conn`) so the cleanup
    /// behavior can be unit-tested without an `AppHandle`. Returns the ids of
    /// the rows that were actually deleted so the caller (which has the
    /// `AppHandle`) can notify the frontend about each removal.
    fn cleanup_old_entries_with_conn(
        conn: &Connection,
        recordings_dir: &Path,
        retention_period: crate::settings::RecordingRetentionPeriod,
        history_limit: usize,
    ) -> Result<Vec<i64>> {
        match retention_period {
            crate::settings::RecordingRetentionPeriod::Never => {
                // Don't delete anything
                Ok(Vec::new())
            }
            crate::settings::RecordingRetentionPeriod::PreserveLimit => {
                // Use the old count-based logic with history_limit
                Self::cleanup_by_count_with_conn(conn, recordings_dir, history_limit)
            }
            _ => {
                // Use time-based logic
                Self::cleanup_by_time_with_conn(conn, recordings_dir, retention_period)
            }
        }
    }

    /// Hook run by the settings commands after a new history limit or
    /// retention period has been persisted.
    ///
    /// Deliberately infallible: the setting is already persisted by the time
    /// this runs, so a failure here (for example a transient `history.db`
    /// open error) would make the command report failure and the frontend
    /// roll back its optimistic value while the backend keeps the new one.
    /// The connection open is therefore best-effort — an error is logged and
    /// the hook is skipped, which is safe because the hook never destroys
    /// data (see `apply_history_settings_change`).
    pub fn on_history_settings_changed(
        &self,
        new_retention_period: crate::settings::RecordingRetentionPeriod,
        new_history_limit: usize,
    ) {
        match self.get_connection() {
            Ok(conn) => Self::apply_history_settings_change(
                &conn,
                &self.recordings_dir,
                new_retention_period,
                new_history_limit,
            ),
            Err(e) => {
                warn!(
                    "Skipping history settings-change hook (could not open history db): {}",
                    e
                );
            }
        }
    }

    /// Everything the settings commands do to stored history data after a new
    /// history limit or retention period is persisted. Extracted with an
    /// explicit connection + recordings dir (same pattern as
    /// `delete_entry_with_conn`) so the behavior is unit-testable without an
    /// `AppHandle`, and given full access to the data on purpose: the
    /// regression test hands it a real database and real WAV files and
    /// asserts they survive.
    ///
    /// Deliberately performs no cleanup. Running an immediate cleanup pass
    /// here permanently deleted unsaved recordings and their WAV files the
    /// moment a user lowered the history limit or changed retention, with no
    /// warning and no undo (#55). Entries beyond the new limit are instead
    /// trimmed lazily by `save_entry` when the next recording is added, so a
    /// settings change alone never destroys data. Any future on-change
    /// behavior belongs here, where the test can see it.
    pub(crate) fn apply_history_settings_change(
        _conn: &Connection,
        _recordings_dir: &Path,
        _new_retention_period: crate::settings::RecordingRetentionPeriod,
        _new_history_limit: usize,
    ) {
    }

    /// Returns the ids of the rows that were actually deleted (a stale entry
    /// saved since selection is skipped and not reported).
    fn delete_entries_and_files(
        conn: &Connection,
        recordings_dir: &Path,
        entries: &[(i64, String)],
    ) -> Result<Vec<i64>> {
        let mut deleted_ids = Vec::new();

        for (id, file_name) in entries {
            // Re-check `saved` at delete time: the entry list was SELECTed
            // earlier, and toggle_saved_status runs on its own connection from
            // another command thread, so the user may have starred an entry in
            // that window. A just-saved recording must never be destroyed by a
            // stale cleanup list, and the WAV file is only unlinked when the
            // row was actually deleted.
            let rows_deleted = conn.execute(
                "DELETE FROM transcription_history WHERE id = ?1 AND saved = 0",
                params![id],
            )?;
            if rows_deleted == 0 {
                debug!("Skipping cleanup of entry {}: saved since selection", id);
                continue;
            }
            deleted_ids.push(*id);

            // Delete WAV file
            let file_path = recordings_dir.join(file_name);
            if file_path.exists() {
                if let Err(e) = fs::remove_file(&file_path) {
                    error!("Failed to delete WAV file {}: {}", file_name, e);
                } else {
                    debug!("Deleted old WAV file: {}", file_name);
                }
            }
        }

        Ok(deleted_ids)
    }

    fn cleanup_by_count_with_conn(
        conn: &Connection,
        recordings_dir: &Path,
        limit: usize,
    ) -> Result<Vec<i64>> {
        // Get all entries that are not saved, ordered by timestamp desc
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history WHERE saved = 0 ORDER BY timestamp DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        if entries.len() > limit {
            let entries_to_delete = &entries[limit..];
            let deleted_ids =
                Self::delete_entries_and_files(conn, recordings_dir, entries_to_delete)?;

            if !deleted_ids.is_empty() {
                debug!(
                    "Cleaned up {} old history entries by count",
                    deleted_ids.len()
                );
            }
            return Ok(deleted_ids);
        }

        Ok(Vec::new())
    }

    fn cleanup_by_time_with_conn(
        conn: &Connection,
        recordings_dir: &Path,
        retention_period: crate::settings::RecordingRetentionPeriod,
    ) -> Result<Vec<i64>> {
        // Calculate cutoff timestamp (current time minus retention period)
        let now = Utc::now().timestamp();
        let cutoff_timestamp = match retention_period {
            crate::settings::RecordingRetentionPeriod::Days3 => now - (3 * 24 * 60 * 60), // 3 days in seconds
            crate::settings::RecordingRetentionPeriod::Weeks2 => now - (2 * 7 * 24 * 60 * 60), // 2 weeks in seconds
            crate::settings::RecordingRetentionPeriod::Months3 => now - (3 * 30 * 24 * 60 * 60), // 3 months in seconds (approximate)
            _ => unreachable!("Should not reach here"),
        };

        // Get all unsaved entries older than the cutoff timestamp
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history WHERE saved = 0 AND timestamp < ?1",
        )?;

        let rows = stmt.query_map(params![cutoff_timestamp], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries_to_delete: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries_to_delete.push(row?);
        }

        let deleted_ids = Self::delete_entries_and_files(conn, recordings_dir, &entries_to_delete)?;

        if !deleted_ids.is_empty() {
            debug!(
                "Cleaned up {} old history entries based on retention period",
                deleted_ids.len()
            );
        }

        Ok(deleted_ids)
    }

    pub async fn get_history_entries(
        &self,
        cursor: Option<i64>,
        limit: Option<usize>,
    ) -> Result<PaginatedHistory> {
        let conn = self.get_connection()?;
        let limit = limit.map(|l| l.min(100));

        let mut entries: Vec<HistoryEntry> = match (cursor, limit) {
            (Some(cursor_id), Some(lim)) => {
                let fetch_count = (lim + 1) as i64;
                let mut stmt = conn.prepare(
                    "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested, raw_requested
                     FROM transcription_history
                     WHERE id < ?1
                     ORDER BY id DESC
                     LIMIT ?2",
                )?;
                let result = stmt
                    .query_map(params![cursor_id, fetch_count], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
            (None, Some(lim)) => {
                let fetch_count = (lim + 1) as i64;
                let mut stmt = conn.prepare(
                    "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested, raw_requested
                     FROM transcription_history
                     ORDER BY id DESC
                     LIMIT ?1",
                )?;
                let result = stmt
                    .query_map(params![fetch_count], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
            (_, None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested, raw_requested
                     FROM transcription_history
                     ORDER BY id DESC",
                )?;
                let result = stmt
                    .query_map([], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
        };

        let has_more = limit.is_some_and(|lim| entries.len() > lim);
        if has_more {
            entries.pop();
        }

        Ok(PaginatedHistory { entries, has_more })
    }

    /// Fetch every stored transcription's raw dictated text -- the `transcription_text` column only,
    /// no audio or metadata -- newest first. Backs issue #16 history mining, which surfaces
    /// frequently-used vocabulary from the user's own transcripts. Empty transcriptions are skipped.
    ///
    /// Deliberately mines `transcription_text`, not `post_processed_text`: post-processing is a
    /// user-defined LLM prompt that can translate, summarize, or otherwise rewrite the transcript, so
    /// its output may contain words the user never dictated. The raw transcription is the faithful
    /// record of what was spoken, which is what custom-word suggestions should be drawn from. (The
    /// tray/display layer separately prefers the post-processed text; that's a different concern.)
    pub fn get_all_transcription_texts(&self) -> Result<Vec<String>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT transcription_text
             FROM transcription_history
             WHERE transcription_text IS NOT NULL AND transcription_text != ''
             ORDER BY id DESC",
        )?;
        let texts = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(texts)
    }

    #[cfg(test)]
    fn get_latest_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                raw_requested
             FROM transcription_history
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    /// Get the latest entry with non-empty transcription text.
    pub fn get_latest_completed_entry(&self) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        Self::get_latest_completed_entry_with_conn(&conn)
    }

    fn get_latest_completed_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                raw_requested
             FROM transcription_history
             WHERE transcription_text != ''
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    pub async fn toggle_saved_status(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;

        // Get current saved status
        let current_saved: bool = conn.query_row(
            "SELECT saved FROM transcription_history WHERE id = ?1",
            params![id],
            |row| row.get("saved"),
        )?;

        let new_saved = !current_saved;

        conn.execute(
            "UPDATE transcription_history SET saved = ?1 WHERE id = ?2",
            params![new_saved, id],
        )?;

        debug!("Toggled saved status for entry {}: {}", id, new_saved);

        // Emit history updated event
        if let Err(e) = (HistoryUpdatePayload::Toggled { id }).emit(&self.app_handle) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    pub fn get_audio_file_path(&self, file_name: &str) -> Result<PathBuf> {
        if !is_safe_recording_filename(file_name) {
            return Err(anyhow!("Invalid recording file name: {file_name}"));
        }
        Ok(self.recordings_dir.join(file_name))
    }

    pub async fn get_entry_by_id(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                raw_requested
             FROM transcription_history
             WHERE id = ?1",
        )?;

        let entry = stmt.query_row([id], Self::map_history_entry).optional()?;

        Ok(entry)
    }

    pub async fn delete_entry(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;

        Self::delete_entry_with_conn(&conn, &self.recordings_dir, id)?;

        debug!("Deleted history entry with id: {}", id);

        // Emit history updated event
        if let Err(e) = (HistoryUpdatePayload::Deleted { id }).emit(&self.app_handle) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    /// Remove a history row and best-effort delete its audio file.
    ///
    /// Extracted with an explicit connection + recordings dir so it can be
    /// unit-tested without an `AppHandle`. The audio file is unlinked only when
    /// the stored `file_name` passes `is_safe_recording_filename`; a corrupted
    /// or tampered name (e.g. one containing path separators) skips the unlink
    /// but the database row is always removed, so a bad row can never get stuck
    /// in history. A failed file unlink is likewise logged and tolerated.
    fn delete_entry_with_conn(conn: &Connection, recordings_dir: &Path, id: i64) -> Result<()> {
        let file_name: Option<String> = conn
            .query_row(
                "SELECT file_name FROM transcription_history WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(file_name) = file_name {
            if is_safe_recording_filename(&file_name) {
                let file_path = recordings_dir.join(&file_name);
                if file_path.exists() {
                    if let Err(e) = fs::remove_file(&file_path) {
                        error!("Failed to delete audio file {}: {}", file_name, e);
                        // Continue with database deletion even if file deletion fails
                    }
                }
            } else {
                warn!(
                    "History entry {} has an invalid file name {:?}; skipping audio-file deletion but removing the row",
                    id, file_name
                );
            }
        }

        conn.execute(
            "DELETE FROM transcription_history WHERE id = ?1",
            params![id],
        )?;

        Ok(())
    }

    fn format_timestamp_title(&self, timestamp: i64) -> String {
        if let Some(utc_datetime) = DateTime::from_timestamp(timestamp, 0) {
            // Convert UTC to local timezone
            let local_datetime = utc_datetime.with_timezone(&Local);
            local_datetime.format("%B %e, %Y - %l:%M%p").to_string()
        } else {
            format!("Recording {}", timestamp)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE transcription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_name TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                saved BOOLEAN NOT NULL DEFAULT 0,
                title TEXT NOT NULL,
                transcription_text TEXT NOT NULL,
                post_processed_text TEXT,
                post_process_prompt TEXT,
                post_process_requested BOOLEAN NOT NULL DEFAULT 0,
                raw_requested BOOLEAN NOT NULL DEFAULT 0
            );",
        )
        .expect("create transcription_history table");
        conn
    }

    fn insert_entry(conn: &Connection, timestamp: i64, text: &str, post_processed: Option<&str>) {
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                raw_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                format!("handy-{}.wav", timestamp),
                timestamp,
                false,
                format!("Recording {}", timestamp),
                text,
                post_processed,
                Option::<String>::None,
                false,
                false,
            ],
        )
        .expect("insert history entry");
    }

    #[test]
    fn get_latest_entry_returns_none_when_empty() {
        let conn = setup_conn();
        let entry = HistoryManager::get_latest_entry_with_conn(&conn).expect("fetch latest entry");
        assert!(entry.is_none());
    }

    #[test]
    fn get_latest_entry_returns_newest_entry() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "first", None);
        insert_entry(&conn, 200, "second", Some("processed"));

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        assert_eq!(entry.timestamp, 200);
        assert_eq!(entry.transcription_text, "second");
        assert_eq!(entry.post_processed_text.as_deref(), Some("processed"));
    }

    #[test]
    fn raw_requested_round_trips() {
        // A raw dictation persists raw_requested = true so a later retry can reproduce it.
        let conn = setup_conn();
        conn.execute(
            "INSERT INTO transcription_history (
                file_name, timestamp, saved, title, transcription_text,
                post_processed_text, post_process_prompt, post_process_requested, raw_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                "handy-300.wav",
                300,
                false,
                "Recording 300",
                "raw entry",
                Option::<String>::None,
                Option::<String>::None,
                false,
                true,
            ],
        )
        .expect("insert raw entry");

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        assert!(entry.raw_requested);
        assert!(!entry.post_process_requested);
    }

    #[test]
    fn get_latest_completed_entry_skips_empty_entries() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "completed", None);
        insert_entry(&conn, 200, "", None);

        let entry = HistoryManager::get_latest_completed_entry_with_conn(&conn)
            .expect("fetch latest completed entry")
            .expect("completed entry exists");

        assert_eq!(entry.timestamp, 100);
        assert_eq!(entry.transcription_text, "completed");
    }

    #[test]
    fn is_safe_recording_filename_accepts_generated_names() {
        // Recording names are always server-generated as handy-<timestamp>.wav.
        assert!(is_safe_recording_filename("handy-1700000000.wav"));
        assert!(is_safe_recording_filename("custom_start.wav"));
    }

    #[test]
    fn is_safe_recording_filename_rejects_traversal_and_absolute() {
        assert!(!is_safe_recording_filename(""));
        assert!(!is_safe_recording_filename(".."));
        assert!(!is_safe_recording_filename("../secret.txt"));
        assert!(!is_safe_recording_filename("..\\..\\Windows\\win.ini"));
        assert!(!is_safe_recording_filename("sub/handy.wav"));
        assert!(!is_safe_recording_filename("sub\\handy.wav"));
        assert!(!is_safe_recording_filename("C:\\Windows\\win.ini"));
        assert!(!is_safe_recording_filename("\\\\server\\share\\payload"));
        assert!(!is_safe_recording_filename("handy\0.wav"));
    }

    fn insert_entry_with_file_name(conn: &Connection, file_name: &str, timestamp: i64) {
        conn.execute(
            "INSERT INTO transcription_history (
                file_name, timestamp, saved, title, transcription_text,
                post_processed_text, post_process_prompt, post_process_requested, raw_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                file_name,
                timestamp,
                false,
                format!("Recording {}", timestamp),
                "text",
                Option::<String>::None,
                Option::<String>::None,
                false,
                false,
            ],
        )
        .expect("insert history entry");
    }

    fn row_count(conn: &Connection, id: i64) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM transcription_history WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .expect("count rows")
    }

    #[test]
    fn delete_entry_removes_row_with_valid_file_name() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "completed", None);
        let dir = std::env::temp_dir();

        // The audio file does not exist on disk, so this only removes the row.
        HistoryManager::delete_entry_with_conn(&conn, &dir, 1).expect("delete entry");

        assert_eq!(row_count(&conn, 1), 0);
    }

    fn all_row_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM transcription_history", [], |row| {
            row.get(0)
        })
        .expect("count all rows")
    }

    /// Seed `count` unsaved entries (timestamps 100, 200, ...) with matching
    /// WAV files on disk, mirroring the state of a real recordings dir.
    fn seed_entries_with_files(conn: &Connection, dir: &Path, count: i64) -> Vec<PathBuf> {
        (1..=count)
            .map(|i| {
                let timestamp = i * 100;
                insert_entry(conn, timestamp, "text", None);
                let path = dir.join(format!("handy-{}.wav", timestamp));
                std::fs::write(&path, b"RIFF").expect("write wav file");
                path
            })
            .collect()
    }

    #[test]
    fn changing_history_settings_does_not_destroy_unsaved_entries() {
        // Issue #55: lowering the history limit (or changing retention) used to
        // run an immediate cleanup pass that permanently deleted unsaved
        // recordings and their WAV files, with no warning and no undo. A
        // settings change must leave stored data untouched; trimming belongs to
        // the lazy cleanup in `save_entry` when the next recording is added.
        let conn = setup_conn();
        let dir = tempfile::tempdir().expect("create temp recordings dir");
        let wav_paths = seed_entries_with_files(&conn, dir.path(), 3);

        // The user lowers the history limit from 3 to 1. This is the same
        // function the settings commands execute after persisting the new
        // value, handed the real database and recordings dir.
        HistoryManager::apply_history_settings_change(
            &conn,
            dir.path(),
            crate::settings::RecordingRetentionPeriod::PreserveLimit,
            1,
        );

        assert_eq!(
            all_row_count(&conn),
            3,
            "a settings change must not delete history rows"
        );
        for path in &wav_paths {
            assert!(
                path.exists(),
                "a settings change must not delete WAV files: {:?}",
                path
            );
        }
    }

    #[test]
    fn cleanup_delete_skips_entries_saved_after_selection() {
        // A cleanup pass SELECTs its unsaved victims first and deletes them
        // afterwards, while toggle_saved_status runs on its own connection
        // from another command thread. If the user stars an entry in that
        // window, the stale list still contains it; the delete must re-check
        // `saved` so a just-saved recording is never destroyed.
        let conn = setup_conn();
        let dir = tempfile::tempdir().expect("create temp recordings dir");
        let wav_paths = seed_entries_with_files(&conn, dir.path(), 2);

        // Entry 1 (timestamp 100) was captured by the cleanup SELECT while
        // unsaved, then the user saved it before the delete ran.
        conn.execute(
            "UPDATE transcription_history SET saved = 1 WHERE timestamp = 100",
            [],
        )
        .expect("mark entry saved");

        let stale_list = vec![
            (1i64, "handy-100.wav".to_string()),
            (2i64, "handy-200.wav".to_string()),
        ];
        let deleted_ids = HistoryManager::delete_entries_and_files(&conn, dir.path(), &stale_list)
            .expect("delete entries");

        // Only the row that was actually deleted is reported; the skipped
        // saved entry must not produce a Deleted event upstream.
        assert_eq!(deleted_ids, vec![2]);

        // The just-saved entry and its WAV survive; the unsaved one is gone.
        assert_eq!(
            row_count(&conn, 1),
            1,
            "a row saved after the cleanup SELECT must survive"
        );
        assert!(
            wav_paths[0].exists(),
            "the WAV of a row saved after the cleanup SELECT must survive"
        );
        assert_eq!(row_count(&conn, 2), 0);
        assert!(!wav_paths[1].exists());
    }

    #[test]
    fn lazy_cleanup_still_trims_unsaved_entries_beyond_limit() {
        // The `save_entry` cleanup path must keep enforcing the limit: with a
        // limit of 1, the two oldest unsaved entries and their files go, the
        // newest stays, and saved entries are never touched.
        let conn = setup_conn();
        let dir = tempfile::tempdir().expect("create temp recordings dir");
        let wav_paths = seed_entries_with_files(&conn, dir.path(), 3);
        // A saved entry older than everything else, which must survive.
        insert_entry(&conn, 50, "saved text", None);
        conn.execute(
            "UPDATE transcription_history SET saved = 1 WHERE timestamp = 50",
            [],
        )
        .expect("mark entry saved");
        let saved_wav = dir.path().join("handy-50.wav");
        std::fs::write(&saved_wav, b"RIFF").expect("write saved wav file");

        let mut deleted_ids = HistoryManager::cleanup_old_entries_with_conn(
            &conn,
            dir.path(),
            crate::settings::RecordingRetentionPeriod::PreserveLimit,
            1,
        )
        .expect("run lazy cleanup");

        // The saved entry plus the newest unsaved entry remain.
        assert_eq!(all_row_count(&conn), 2);
        assert!(!wav_paths[0].exists(), "oldest unsaved WAV should be gone");
        assert!(!wav_paths[1].exists(), "older unsaved WAV should be gone");
        assert!(wav_paths[2].exists(), "newest unsaved WAV should remain");
        assert!(saved_wav.exists(), "saved WAV must never be deleted");

        // The trimmed row ids are reported so `cleanup_old_entries` can emit a
        // Deleted event per row and a mounted History page drops them. The
        // emit itself needs an AppHandle and is not unit-testable; this pins
        // the signal it is driven by. Ids 1 and 2 are the two oldest unsaved
        // entries (timestamps 100 and 200).
        deleted_ids.sort_unstable();
        assert_eq!(deleted_ids, vec![1, 2]);
    }

    #[test]
    fn delete_entry_removes_row_with_invalid_file_name() {
        // A tampered/corrupted row with a traversal file_name must still be
        // deletable: the audio-file unlink is skipped, but the row is removed
        // so the user is not stuck with an undeletable history entry.
        let conn = setup_conn();
        insert_entry_with_file_name(&conn, "../../evil.wav", 200);
        let dir = std::env::temp_dir();

        HistoryManager::delete_entry_with_conn(&conn, &dir, 1)
            .expect("delete entry with invalid file name");

        assert_eq!(row_count(&conn, 1), 0);
    }
}
