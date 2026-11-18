//! Files on the system clipboard, in the same form Finder uses, so ⌘C in
//! Finder and ⌘V in Patina work (and the other way round). Also plain text,
//! for Copy Path.

#[derive(Default, serde::Serialize)]
pub struct PastedFiles {
    pub paths: Vec<String>,
    pub cut: bool,
}

#[cfg(target_os = "macos")]
mod mac {
    use super::PastedFiles;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};

    #[derive(Default)]
    pub struct FileClipboard {
        cut_change: Option<isize>,
    }

    pub static FILES: std::sync::Mutex<FileClipboard> =
        std::sync::Mutex::new(FileClipboard { cut_change: None });

    impl FileClipboard {
        pub fn copy(&mut self, pb: &NSPasteboard, paths: &[String], cut: bool) -> bool {
            let change = write(pb, paths);
            self.cut_change = change.filter(|_| cut);
            change.is_some()
        }

        pub fn paste(&mut self, pb: &NSPasteboard) -> PastedFiles {
            let change = pb.changeCount();
            let paths = read(pb);
            let cut = self.cut_change.take() == Some(change) && pb.changeCount() == change;
            PastedFiles { paths, cut }
        }
    }

    fn write(pb: &NSPasteboard, paths: &[String]) -> Option<isize> {
        let urls: Vec<Retained<NSURL>> =
            paths.iter().map(|p| NSURL::fileURLWithPath(&NSString::from_str(p))).collect();
        let objects: Vec<&ProtocolObject<dyn NSPasteboardWriting>> =
            urls.iter().map(|u| ProtocolObject::from_ref(&**u)).collect();
        // clearContents returns the change count of our write. A later Finder
        // copy receives a new count even when it contains the same URLs.
        let change = pb.clearContents();
        pb.writeObjects(&NSArray::from_slice(&objects)).then_some(change)
    }

    fn read(pb: &NSPasteboard) -> Vec<String> {
        let Some(items) = pb.pasteboardItems() else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| item.stringForType(unsafe { NSPasteboardTypeFileURL }))
            .filter_map(|url| NSURL::URLWithString(&url))
            // Some apps copy file *reference* URLs (file:///.file/id=…);
            // filePathURL turns those into ordinary paths.
            .filter_map(|url| url.filePathURL())
            .filter_map(|url| url.path())
            .map(|path| path.to_string())
            .collect()
    }

    pub fn write_text(pb: &NSPasteboard, text: &str) -> bool {
        pb.clearContents();
        pb.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString })
    }

    #[cfg(test)]
    pub fn read_text(pb: &NSPasteboard) -> Option<String> {
        pb.stringForType(unsafe { NSPasteboardTypeString }).map(|s| s.to_string())
    }

    pub fn general() -> Retained<NSPasteboard> {
        NSPasteboard::generalPasteboard()
    }
}

/// Puts these files on the clipboard, replacing what was there.
#[cfg(target_os = "macos")]
pub fn copy_files(paths: &[String], cut: bool) -> bool {
    mac::FILES.lock().unwrap().copy(&mac::general(), paths, cut)
}

/// The files on the clipboard (from Patina, Finder or any other app).
#[cfg(target_os = "macos")]
pub fn pasted_files() -> PastedFiles {
    mac::FILES.lock().unwrap().paste(&mac::general())
}

/// Puts plain text on the clipboard, replacing what was there.
#[cfg(target_os = "macos")]
pub fn copy_text(text: &str) -> bool {
    mac::write_text(&mac::general(), text)
}

#[cfg(not(target_os = "macos"))]
pub fn copy_files(_paths: &[String], _cut: bool) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn pasted_files() -> PastedFiles {
    PastedFiles::default()
}

#[cfg(not(target_os = "macos"))]
pub fn copy_text(_text: &str) -> bool {
    false
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::mac;
    use objc2_app_kit::NSPasteboard;

    #[test]
    fn files_round_trip_through_a_pasteboard() {
        // A private pasteboard, so the test never touches the real clipboard.
        let pb = NSPasteboard::pasteboardWithUniqueName();
        let mut clipboard = mac::FileClipboard::default();
        let paths = vec!["/tmp/one file.txt".to_string(), "/Applications".to_string()];
        assert!(clipboard.copy(&pb, &paths, false));
        assert_eq!(clipboard.paste(&pb).paths, paths);
        assert!(clipboard.copy(&pb, &["/tmp".to_string()], false));
        assert_eq!(clipboard.paste(&pb).paths, ["/tmp"]);
    }

    #[test]
    fn a_cut_is_consumed_once_and_a_new_copy_of_the_same_paths_pastes_as_a_copy() {
        let pb = NSPasteboard::pasteboardWithUniqueName();
        let paths = vec!["/tmp/report.pdf".to_string()];
        let mut patina = mac::FileClipboard::default();
        assert!(patina.copy(&pb, &paths, true));
        assert!(patina.paste(&pb).cut);
        assert!(!patina.paste(&pb).cut);
        assert!(patina.copy(&pb, &paths, true));
        let mut finder = mac::FileClipboard::default();
        assert!(finder.copy(&pb, &paths, false));
        let paste = patina.paste(&pb);
        assert_eq!(paste.paths, paths);
        assert!(!paste.cut);
    }

    #[test]
    fn text_replaces_files_on_a_pasteboard() {
        let pb = NSPasteboard::pasteboardWithUniqueName();
        let mut clipboard = mac::FileClipboard::default();
        assert!(clipboard.copy(&pb, &["/tmp".to_string()], true));
        let paths = "/Users/me/one file.txt\n/Applications";
        assert!(mac::write_text(&pb, paths));
        assert_eq!(mac::read_text(&pb).as_deref(), Some(paths));
        // Text isn't a file, so pasting files finds none.
        let paste = clipboard.paste(&pb);
        assert!(paste.paths.is_empty());
        assert!(!paste.cut);
    }
}
