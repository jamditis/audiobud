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
    /// May add the file list without clearing formats already on the
    /// clipboard (Windows CF_HDROP is set additively). Callers that need a
    /// clean state must call `clear` first.
    fn write_files(&mut self, files: &ClipboardFiles) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
}

/// Reads a snapshot of the current clipboard contents. A format that cannot
/// be read (absent, or unsupported on this platform) is captured as `None`.
pub fn capture(backend: &mut dyn ClipboardBackend) -> ClipboardContent {
    ClipboardContent {
        text: backend.read_text(),
        html: backend.read_html(),
        image: backend.read_image(),
        files: backend.read_files(),
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
/// - A snapshot holding both an image and text (e.g. a spreadsheet range
///   copy) restores the text/HTML side and drops the image render.
/// - Cut versus copy: on Windows the "Preferred DropEffect" marker is
///   captured and restored, so a cut (move) file list stays a cut. On Linux
///   the KDE/GNOME cut markers (`application/x-kde-cutselection`,
///   `x-special/gnome-copied-files`) are not reachable through arboard, so a
///   restored cut degrades to a copy and a later paste duplicates the files
///   instead of moving them. macOS has no cut marker on the pasteboard (move
///   is chosen at paste time), so nothing is lost there.
pub fn restore(
    backend: &mut dyn ClipboardBackend,
    content: &ClipboardContent,
) -> Result<(), String> {
    if let Some(files) = &content.files {
        // The file-list write can be additive (Windows CF_HDROP is set
        // without emptying the clipboard), which would leave the transcript
        // text alongside the restored files. Clear first.
        backend.clear()?;
        return backend.write_files(files);
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
        Some(ClipboardFiles {
            paths,
            #[cfg(windows)]
            preferred_drop_effect: windows_drop_effect::read(),
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
        self.0
            .set()
            .file_list(&files.paths)
            .map_err(|e| format!("Failed to restore clipboard file list: {}", e))?;

        #[cfg(windows)]
        if let Some(effect) = files.preferred_drop_effect {
            windows_drop_effect::write(effect)?;
        }

        Ok(())
    }

    fn clear(&mut self) -> Result<(), String> {
        self.0
            .clear()
            .map_err(|e| format!("Failed to clear clipboard: {}", e))
    }
}

/// Reads and writes the Windows "Preferred DropEffect" clipboard format,
/// which Explorer uses to distinguish cut (move) from copied file lists.
/// arboard does not expose it, so this goes through clipboard-win directly
/// (the same crate arboard uses underneath).
#[cfg(windows)]
mod windows_drop_effect {
    const FORMAT_NAME: &str = "Preferred DropEffect";

    /// Reads the marker off the current clipboard, if present.
    pub fn read() -> Option<u32> {
        let format = clipboard_win::register_format(FORMAT_NAME)?;
        let _open = clipboard_win::Clipboard::new_attempts(10).ok()?;
        if !clipboard_win::is_format_avail(format.get()) {
            return None;
        }
        let mut out = Vec::new();
        clipboard_win::raw::get_vec(format.get(), &mut out).ok()?;
        let bytes: [u8; 4] = out.get(..4)?.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    /// Adds the marker to the clipboard without clearing the formats already
    /// on it (the file list written just before).
    pub fn write(effect: u32) -> Result<(), String> {
        let format = clipboard_win::register_format(FORMAT_NAME)
            .ok_or_else(|| "Failed to register the Preferred DropEffect format".to_string())?;
        let _open = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|e| format!("Failed to open clipboard for the drop effect: {}", e))?;
        clipboard_win::raw::set_without_clear(format.get(), &effect.to_le_bytes())
            .map_err(|e| format!("Failed to restore the Preferred DropEffect: {}", e))
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
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&mut self) -> Option<String> {
            self.text.clone()
        }

        fn read_html(&mut self) -> Option<String> {
            self.html.clone()
        }

        fn read_image(&mut self) -> Option<ClipboardImage> {
            self.image.clone()
        }

        fn read_files(&mut self) -> Option<ClipboardFiles> {
            self.files.clone()
        }

        fn write_text(&mut self, text: &str) -> Result<(), String> {
            *self = Self::default();
            self.text = Some(text.to_string());
            Ok(())
        }

        fn write_html(&mut self, html: &str, alt_text: Option<&str>) -> Result<(), String> {
            *self = Self::default();
            self.html = Some(html.to_string());
            self.text = alt_text.map(str::to_string);
            Ok(())
        }

        fn write_image(&mut self, image: &ClipboardImage) -> Result<(), String> {
            *self = Self::default();
            self.image = Some(image.clone());
            Ok(())
        }

        fn write_files(&mut self, files: &ClipboardFiles) -> Result<(), String> {
            // Models the Windows behavior: CF_HDROP is added via
            // SetClipboardData without emptying the clipboard first, so
            // whatever text/HTML/image was there survives alongside it.
            self.files = Some(files.clone());
            Ok(())
        }

        fn clear(&mut self) -> Result<(), String> {
            *self = Self::default();
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
        // The file-list write is additive (Windows CF_HDROP does not empty
        // the clipboard), so the restore path must clear the transcript
        // first or text-aware apps keep pasting it.
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
