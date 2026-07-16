//! Full-content clipboard snapshot and restore (issue #57).
//!
//! `paste_via_clipboard` overwrites the clipboard to inject the transcript,
//! then puts the user's original content back. Saving only the text
//! representation destroys images, HTML, and file lists. This module captures
//! every format arboard exposes and restores the richest one.

use std::borrow::Cow;
use std::path::PathBuf;

/// An RGBA8 image captured from the clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    /// RGBA8 pixel data, row-major, `width * height * 4` bytes.
    pub bytes: Vec<u8>,
}

/// A file list captured from the clipboard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    /// Windows "Preferred DropEffect" marker distinguishing a cut
    /// (DROPEFFECT_MOVE) from a copy (DROPEFFECT_COPY), captured as the raw
    /// DWORD. `None` when absent or on platforms without the concept.
    /// Without it, restoring a cut file list would turn the pending move
    /// into a copy and a later paste would duplicate the files.
    pub preferred_drop_effect: Option<u32>,
}

/// Everything readable off the clipboard before overwriting it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipboardContent {
    pub text: Option<String>,
    pub html: Option<String>,
    pub image: Option<ClipboardImage>,
    pub files: Option<ClipboardFiles>,
}

impl ClipboardContent {
    /// True when the snapshot holds text and nothing else. Used on Wayland to
    /// route the restore through wl-copy, matching the transcript write path.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn is_text_only(&self) -> bool {
        self.text.is_some() && self.html.is_none() && self.image.is_none() && self.files.is_none()
    }
}

/// Seam over the OS clipboard so capture/restore logic is testable without
/// touching the real clipboard.
pub trait ClipboardBackend {
    fn read_text(&mut self) -> Option<String>;
    fn read_html(&mut self) -> Option<String>;
    fn read_image(&mut self) -> Option<ClipboardImage>;
    fn read_files(&mut self) -> Option<ClipboardFiles>;
    fn write_text(&mut self, text: &str) -> Result<(), String>;
    /// Writes HTML plus an optional plain-text alternate in one clipboard state.
    fn write_html(&mut self, html: &str, alt_text: Option<&str>) -> Result<(), String>;
    fn write_image(&mut self, image: &ClipboardImage) -> Result<(), String>;
    /// Replaces the whole clipboard state with the file list (and its
    /// cut/copy marker where the platform has one), like the other write
    /// methods. Nothing written before it (e.g. the transcript) may survive.
    fn write_files(&mut self, files: &ClipboardFiles) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
}

/// Reads a snapshot of the current clipboard contents. A format that cannot
/// be read (absent, or unsupported on this platform) is captured as `None`.
///
/// Capture order: text, HTML, and the file list are read first, and the
/// image only when both text and files are absent. [`restore`] can only ever
/// write the image back in that case, and decoding a clipboard image is the
/// one expensive read (a large spreadsheet selection renders to hundreds of
/// MB of RGBA), so it is skipped whenever it could not be restored.
pub fn capture(backend: &mut dyn ClipboardBackend) -> ClipboardContent {
    let text = backend.read_text();
    let html = backend.read_html();
    let files = backend.read_files();
    let image = if text.is_none() && files.is_none() {
        backend.read_image()
    } else {
        None
    };
    ClipboardContent {
        text,
        html,
        image,
        files,
    }
}

/// Restores a previously captured snapshot.
///
/// arboard writes one clipboard state per set call, so a snapshot holding
/// several formats restores the richest one: file list, then image (when no
/// text was captured), then HTML with the text as its plain-text alternate,
/// then text. An empty snapshot clears the clipboard instead of writing an
/// empty string.
///
/// Known limitations:
/// - A clipboard holding both an image and text (e.g. a spreadsheet range
///   copy) restores the text/HTML side; the image render is dropped (it is
///   not even captured, see [`capture`]).
/// - Cut versus copy: on Windows the "Preferred DropEffect" marker is
///   captured and restored, so a cut (move) file list stays a cut. On Linux
///   the KDE/GNOME cut markers (`application/x-kde-cutselection`,
///   `x-special/gnome-copied-files`) are not reachable through arboard, so a
///   restored cut degrades to a copy and a later paste duplicates the files
///   instead of moving them. macOS has no cut marker on the pasteboard (move
///   is chosen at paste time), so nothing is lost there.
/// - On Linux, arboard canonicalizes each path when writing the file list, so
///   a copied symlink restores as its target — and a broken symlink cannot be
///   restored at all. A failed file-list write falls back to the captured
///   text/HTML (or clearing), so the transcript never stays on the clipboard.
pub fn restore(
    backend: &mut dyn ClipboardBackend,
    content: &ClipboardContent,
) -> Result<(), String> {
    if let Some(files) = &content.files {
        match backend.write_files(files) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // Fall through to the text/HTML formats (or clear): leaving
                // the transcript on the clipboard is worse than degrading
                // the restored fidelity.
                log::warn!(
                    "Failed to restore the clipboard file list, falling back: {}",
                    e
                );
            }
        }
    }
    if content.text.is_none() {
        if let Some(image) = &content.image {
            return backend.write_image(image);
        }
    }
    if let Some(html) = &content.html {
        return backend.write_html(html, content.text.as_deref());
    }
    if let Some(text) = &content.text {
        return backend.write_text(text);
    }
    backend.clear()
}

/// [`ClipboardBackend`] backed by the OS clipboard via arboard.
///
/// The instance is short-lived (created per paste). That is safe on X11
/// because tauri-plugin-clipboard-manager holds a process-lifetime arboard
/// instance, which keeps arboard's shared clipboard server thread (and the
/// restored content) alive after this instance drops.
pub struct ArboardBackend(arboard::Clipboard);

impl ArboardBackend {
    pub fn new() -> Result<Self, String> {
        arboard::Clipboard::new()
            .map(Self)
            .map_err(|e| format!("Failed to open system clipboard: {}", e))
    }
}

impl ClipboardBackend for ArboardBackend {
    fn read_text(&mut self) -> Option<String> {
        self.0.get().text().ok()
    }

    fn read_html(&mut self) -> Option<String> {
        self.0.get().html().ok()
    }

    fn read_image(&mut self) -> Option<ClipboardImage> {
        let image = self.0.get().image().ok()?;
        Some(ClipboardImage {
            width: image.width,
            height: image.height,
            bytes: image.bytes.into_owned(),
        })
    }

    fn read_files(&mut self) -> Option<ClipboardFiles> {
        let paths = self.0.get().file_list().ok().filter(|f| !f.is_empty())?;
        #[cfg(target_os = "linux")]
        let paths = paths.into_iter().map(strip_uri_list_cr).collect();
        Some(ClipboardFiles {
            paths,
            #[cfg(windows)]
            preferred_drop_effect: windows_files::read_drop_effect(),
            #[cfg(not(windows))]
            preferred_drop_effect: None,
        })
    }

    fn write_text(&mut self, text: &str) -> Result<(), String> {
        self.0
            .set()
            .text(text)
            .map_err(|e| format!("Failed to restore clipboard text: {}", e))
    }

    fn write_html(&mut self, html: &str, alt_text: Option<&str>) -> Result<(), String> {
        self.0
            .set()
            .html(html, alt_text)
            .map_err(|e| format!("Failed to restore clipboard HTML: {}", e))
    }

    fn write_image(&mut self, image: &ClipboardImage) -> Result<(), String> {
        self.0
            .set()
            .image(arboard::ImageData {
                width: image.width,
                height: image.height,
                bytes: Cow::Borrowed(&image.bytes),
            })
            .map_err(|e| format!("Failed to restore clipboard image: {}", e))
    }

    fn write_files(&mut self, files: &ClipboardFiles) -> Result<(), String> {
        // On Windows, CF_HDROP and the Preferred DropEffect marker must land
        // in one clipboard transaction: a clipboard listener (clipboard
        // history, third-party managers) can react to the CF_HDROP update in
        // the gap between two opens and the marker would be lost, turning a
        // cut back into a copy. arboard closes the clipboard after
        // `file_list`, so the Windows path goes through clipboard-win
        // directly under a single open. arboard's macOS and Linux setters
        // already replace the clipboard state in one operation.
        #[cfg(windows)]
        {
            windows_files::write(files)
        }

        #[cfg(not(windows))]
        {
            self.0
                .set()
                .file_list(&files.paths)
                .map_err(|e| format!("Failed to restore clipboard file list: {}", e))
        }
    }

    fn clear(&mut self) -> Result<(), String> {
        self.0
            .clear()
            .map_err(|e| format!("Failed to clear clipboard: {}", e))
    }
}

/// arboard 3.6.1 splits `text/uri-list` on `\n` only, so a CRLF-delimited
/// list (the RFC 2483 form GTK and KDE write) leaves a trailing `\r` on every
/// path. Writing such a path back fails and would strand the transcript on
/// the clipboard, so the artifact is stripped at capture time. A real file
/// name ending in `\r` is indistinguishable from the artifact here; upstream
/// arboard accepts the same trade-off in its (unreleased) fix.
#[cfg(target_os = "linux")]
fn strip_uri_list_cr(path: PathBuf) -> PathBuf {
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    match path.as_os_str().as_bytes().strip_suffix(b"\r") {
        Some(stripped) => PathBuf::from(OsString::from_vec(stripped.to_vec())),
        None => path,
    }
}

/// Windows file-list clipboard access through clipboard-win (the same crate
/// arboard uses underneath). Two things arboard cannot do:
/// - the "Preferred DropEffect" marker Explorer uses to distinguish cut
///   (move) from copied file lists is not exposed by arboard at all;
/// - restoring must write CF_HDROP and the marker under one clipboard open,
///   which arboard prevents by closing the clipboard after `file_list`.
#[cfg(windows)]
mod windows_files {
    use super::ClipboardFiles;

    const FORMAT_NAME: &str = "Preferred DropEffect";
    const OPEN_ATTEMPTS: usize = 10;

    /// Reads the drop-effect marker off the current clipboard, if present.
    pub fn read_drop_effect() -> Option<u32> {
        let format = clipboard_win::register_format(FORMAT_NAME)?;
        let _open = clipboard_win::Clipboard::new_attempts(OPEN_ATTEMPTS).ok()?;
        if !clipboard_win::is_format_avail(format.get()) {
            return None;
        }
        let mut out = Vec::new();
        clipboard_win::raw::get_vec(format.get(), &mut out).ok()?;
        let bytes: [u8; 4] = out.get(..4)?.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    /// Builds the CF_HDROP payload: a 20-byte DROPFILES header (offset to
    /// the path block, drop point, non-client flag, fWide=1) followed by
    /// each path as a null-terminated UTF-16 string and a final extra null.
    ///
    /// Built from the path's raw UTF-16 code units rather than through
    /// `to_str()`: CF_HDROP names captured off the clipboard can contain
    /// unpaired surrogates (valid in NTFS names), which `to_str()` rejects —
    /// and a restore failure here strands the transcript on the clipboard.
    pub(super) fn hdrop_buffer(paths: &[std::path::PathBuf]) -> Vec<u8> {
        use std::os::windows::ffi::OsStrExt;
        const DROPFILES_HEADER_LEN: u32 = 20;
        let mut buf = Vec::new();
        buf.extend_from_slice(&DROPFILES_HEADER_LEN.to_le_bytes()); // pFiles
        buf.extend_from_slice(&0i32.to_le_bytes()); // pt.x
        buf.extend_from_slice(&0i32.to_le_bytes()); // pt.y
        buf.extend_from_slice(&0i32.to_le_bytes()); // fNC
        buf.extend_from_slice(&1i32.to_le_bytes()); // fWide
        for path in paths {
            for unit in path.as_os_str().encode_wide() {
                buf.extend_from_slice(&unit.to_le_bytes());
            }
            buf.extend_from_slice(&0u16.to_le_bytes());
        }
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf
    }

    /// Replaces the clipboard with the file list and its drop-effect marker
    /// in a single open/empty/set transaction.
    pub fn write(files: &ClipboardFiles) -> Result<(), String> {
        let hdrop = hdrop_buffer(&files.paths);

        let _open = clipboard_win::Clipboard::new_attempts(OPEN_ATTEMPTS)
            .map_err(|e| format!("Failed to open clipboard for the file list: {}", e))?;

        // The explicit empty replaces the transcript in the same
        // transaction; without it CF_HDROP would sit alongside the
        // transcript's CF_UNICODETEXT instead of displacing it.
        clipboard_win::raw::empty()
            .map_err(|e| format!("Failed to clear the clipboard for the file list: {}", e))?;
        clipboard_win::raw::set_without_clear(clipboard_win::formats::CF_HDROP, &hdrop)
            .map_err(|e| format!("Failed to restore clipboard file list: {}", e))?;

        if let Some(effect) = files.preferred_drop_effect {
            let format = clipboard_win::register_format(FORMAT_NAME)
                .ok_or_else(|| "Failed to register the Preferred DropEffect format".to_string())?;
            clipboard_win::raw::set_without_clear(format.get(), &effect.to_le_bytes())
                .map_err(|e| format!("Failed to restore the Preferred DropEffect: {}", e))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory clipboard with real replace semantics: every write replaces
    /// the whole clipboard state, like the OS clipboard does.
    #[derive(Default)]
    struct FakeClipboard {
        text: Option<String>,
        html: Option<String>,
        image: Option<ClipboardImage>,
        files: Option<ClipboardFiles>,
        /// How many times `read_image` was called. Image reads decode the
        /// full bitmap, so capture must skip them when nothing could be
        /// restored anyway.
        image_reads: usize,
        /// Makes `write_files` fail, modeling e.g. arboard's Linux
        /// canonicalization failing on a broken symlink.
        fail_file_writes: bool,
    }

    impl FakeClipboard {
        /// Clears the clipboard content, keeping test instrumentation
        /// (`image_reads`) intact.
        fn reset_content(&mut self) {
            self.text = None;
            self.html = None;
            self.image = None;
            self.files = None;
        }
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&mut self) -> Option<String> {
            self.text.clone()
        }

        fn read_html(&mut self) -> Option<String> {
            self.html.clone()
        }

        fn read_image(&mut self) -> Option<ClipboardImage> {
            self.image_reads += 1;
            self.image.clone()
        }

        fn read_files(&mut self) -> Option<ClipboardFiles> {
            self.files.clone()
        }

        fn write_text(&mut self, text: &str) -> Result<(), String> {
            self.reset_content();
            self.text = Some(text.to_string());
            Ok(())
        }

        fn write_html(&mut self, html: &str, alt_text: Option<&str>) -> Result<(), String> {
            self.reset_content();
            self.html = Some(html.to_string());
            self.text = alt_text.map(str::to_string);
            Ok(())
        }

        fn write_image(&mut self, image: &ClipboardImage) -> Result<(), String> {
            self.reset_content();
            self.image = Some(image.clone());
            Ok(())
        }

        fn write_files(&mut self, files: &ClipboardFiles) -> Result<(), String> {
            if self.fail_file_writes {
                return Err("file list write failed".to_string());
            }
            // Models the trait contract: the file list replaces the whole
            // clipboard state (Windows does empty + CF_HDROP + drop effect
            // in one transaction; macOS/Linux setters replace implicitly).
            self.reset_content();
            self.files = Some(files.clone());
            Ok(())
        }

        fn clear(&mut self) -> Result<(), String> {
            self.reset_content();
            Ok(())
        }
    }

    fn test_image() -> ClipboardImage {
        ClipboardImage {
            width: 2,
            height: 1,
            bytes: vec![255, 0, 0, 255, 0, 255, 0, 255],
        }
    }

    /// Simulates the paste round-trip: capture, overwrite with the
    /// transcript, restore.
    fn round_trip(clipboard: &mut FakeClipboard) {
        let snapshot = capture(clipboard);
        clipboard.write_text("the transcript").unwrap();
        restore(clipboard, &snapshot).unwrap();
    }

    #[test]
    fn image_only_clipboard_survives_paste_round_trip() {
        let mut clipboard = FakeClipboard {
            image: Some(test_image()),
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.image, Some(test_image()));
        // An image-only clipboard must not gain a text entry.
        assert_eq!(clipboard.text, None);
    }

    #[test]
    fn empty_clipboard_stays_empty_after_paste_round_trip() {
        let mut clipboard = FakeClipboard::default();

        round_trip(&mut clipboard);

        // Restoring an empty snapshot must clear, not write an empty string.
        assert_eq!(clipboard.text, None);
        assert_eq!(clipboard.html, None);
        assert_eq!(clipboard.image, None);
        assert_eq!(clipboard.files, None);
    }

    #[test]
    fn plain_text_clipboard_survives_paste_round_trip() {
        let mut clipboard = FakeClipboard {
            text: Some("original text".to_string()),
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.text, Some("original text".to_string()));
    }

    #[test]
    fn html_clipboard_restores_html_with_text_alternate() {
        let mut clipboard = FakeClipboard {
            html: Some("<b>hello</b>".to_string()),
            text: Some("hello".to_string()),
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.html, Some("<b>hello</b>".to_string()));
        assert_eq!(clipboard.text, Some("hello".to_string()));
    }

    #[test]
    fn file_list_clipboard_survives_paste_round_trip() {
        let files = ClipboardFiles {
            paths: vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.png")],
            preferred_drop_effect: None,
        };
        let mut clipboard = FakeClipboard {
            files: Some(files.clone()),
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.files, Some(files));
    }

    #[test]
    fn file_list_restore_does_not_leave_transcript_text_behind() {
        // Restoring a file list must not leave the transcript pasteable:
        // text-aware apps would keep pasting it (on Windows, CF_HDROP alone
        // would not displace CF_UNICODETEXT without the explicit empty).
        let files = ClipboardFiles {
            paths: vec![PathBuf::from("/tmp/a.txt")],
            preferred_drop_effect: None,
        };
        let mut clipboard = FakeClipboard {
            files: Some(files.clone()),
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.files, Some(files));
        assert_eq!(clipboard.text, None);
        assert_eq!(clipboard.html, None);
        assert_eq!(clipboard.image, None);
    }

    #[test]
    fn failed_file_list_restore_falls_back_and_drops_the_transcript() {
        // A file-list write can fail after the transcript already overwrote
        // the clipboard (e.g. arboard canonicalizing a broken symlink on
        // Linux). The restore must degrade to the captured text rather than
        // leave the transcript pasteable.
        let mut clipboard = FakeClipboard {
            files: Some(ClipboardFiles {
                paths: vec![PathBuf::from("/tmp/broken-link")],
                preferred_drop_effect: None,
            }),
            text: Some("/tmp/broken-link".to_string()),
            fail_file_writes: true,
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.text, Some("/tmp/broken-link".to_string()));
        assert_eq!(clipboard.files, None);
    }

    #[test]
    fn failed_file_list_restore_with_nothing_else_clears_the_transcript() {
        let mut clipboard = FakeClipboard {
            files: Some(ClipboardFiles {
                paths: vec![PathBuf::from("/tmp/broken-link")],
                preferred_drop_effect: None,
            }),
            fail_file_writes: true,
            ..Default::default()
        };

        round_trip(&mut clipboard);

        // Worse than a lost file list is the transcript staying pasteable.
        assert_eq!(clipboard.text, None);
        assert_eq!(clipboard.files, None);
    }

    #[test]
    fn cut_file_list_round_trip_preserves_the_move_marker() {
        // DROPEFFECT_MOVE = 2: an Explorer "cut". Losing the marker would
        // turn the pending move into a copy and duplicate the files.
        let files = ClipboardFiles {
            paths: vec![PathBuf::from("/tmp/a.txt")],
            preferred_drop_effect: Some(2),
        };
        let mut clipboard = FakeClipboard {
            files: Some(files.clone()),
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.files, Some(files));
    }

    #[test]
    fn image_with_html_and_no_text_restores_the_image() {
        // Browser "copy image" puts a bitmap plus an <img> HTML fragment on
        // the clipboard. The bitmap is the payload apps paste.
        let mut clipboard = FakeClipboard {
            image: Some(test_image()),
            html: Some("<img src=\"https://example.com/x.png\">".to_string()),
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.image, Some(test_image()));
    }

    #[test]
    fn image_alongside_text_restores_the_text_side() {
        // Documented limitation: arboard writes one clipboard state per set,
        // so a snapshot holding both an image and text (e.g. a spreadsheet
        // range copy) restores the text and drops the image render.
        let mut clipboard = FakeClipboard {
            image: Some(test_image()),
            text: Some("A1\tB1".to_string()),
            ..Default::default()
        };

        round_trip(&mut clipboard);

        assert_eq!(clipboard.text, Some("A1\tB1".to_string()));
    }

    #[test]
    fn capture_skips_the_image_read_when_text_is_present() {
        // restore() can never write the image back when text was captured
        // (single-set limitation), and decoding a clipboard image can cost
        // hundreds of MB for large spreadsheet selections. Don't read it.
        let mut clipboard = FakeClipboard {
            text: Some("A1\tB1".to_string()),
            image: Some(test_image()),
            ..Default::default()
        };

        let snapshot = capture(&mut clipboard);

        assert_eq!(clipboard.image_reads, 0);
        assert_eq!(snapshot.image, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn capture_strips_the_uri_list_cr_artifact() {
        // arboard 3.6.1 leaves the CRLF `\r` on paths parsed from a
        // CRLF-delimited text/uri-list; restoring `/tmp/a.txt\r` fails and
        // strands the transcript on the clipboard.
        assert_eq!(
            strip_uri_list_cr(PathBuf::from("/tmp/a.txt\r")),
            PathBuf::from("/tmp/a.txt")
        );
        assert_eq!(
            strip_uri_list_cr(PathBuf::from("/tmp/clean.txt")),
            PathBuf::from("/tmp/clean.txt")
        );
    }

    #[cfg(windows)]
    #[test]
    fn hdrop_buffer_encodes_paths_as_utf16_with_terminators() {
        let paths = vec![PathBuf::from("C:\\a.txt")];
        let buf = super::windows_files::hdrop_buffer(&paths);

        // DROPFILES header: pFiles=20, pt=(0,0), fNC=0, fWide=1.
        assert_eq!(&buf[0..4], &20u32.to_le_bytes());
        assert_eq!(&buf[16..20], &1i32.to_le_bytes());

        let units: Vec<u16> = buf[20..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let mut expected: Vec<u16> = "C:\\a.txt".encode_utf16().collect();
        expected.push(0); // path terminator
        expected.push(0); // list terminator
        assert_eq!(units, expected);
    }

    #[cfg(windows)]
    #[test]
    fn hdrop_buffer_preserves_unpaired_surrogates() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        // A lone high surrogate is a valid NTFS name element but not valid
        // Unicode. `to_str()` rejects it — the conversion the old writer
        // used — so restoration would fail after the transcript already
        // overwrote the clipboard.
        let wide: Vec<u16> = "C:\\x".encode_utf16().chain([0xD800]).collect();
        let path = PathBuf::from(OsString::from_wide(&wide));
        assert!(path.to_str().is_none());

        let buf = super::windows_files::hdrop_buffer(std::slice::from_ref(&path));
        let units: Vec<u16> = buf[20..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let mut expected = wide.clone();
        expected.push(0);
        expected.push(0);
        assert_eq!(units, expected);
    }

    #[test]
    fn is_text_only_requires_exactly_text() {
        assert!(ClipboardContent {
            text: Some("t".to_string()),
            ..Default::default()
        }
        .is_text_only());
        assert!(!ClipboardContent::default().is_text_only());
        assert!(!ClipboardContent {
            text: Some("t".to_string()),
            image: Some(test_image()),
            ..Default::default()
        }
        .is_text_only());
    }
}
