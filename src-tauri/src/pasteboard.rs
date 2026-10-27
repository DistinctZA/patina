//! Files on the system clipboard, in the same form Finder uses, so ⌘C in
//! Finder and ⌘V in Patina work (and the other way round).

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};

    pub fn write(pb: &NSPasteboard, paths: &[String]) -> bool {
        let urls: Vec<Retained<NSURL>> =
            paths.iter().map(|p| NSURL::fileURLWithPath(&NSString::from_str(p))).collect();
        let objects: Vec<&ProtocolObject<dyn NSPasteboardWriting>> =
            urls.iter().map(|u| ProtocolObject::from_ref(&**u)).collect();
        pb.clearContents();
        pb.writeObjects(&NSArray::from_slice(&objects))
    }

    pub fn read(pb: &NSPasteboard) -> Vec<String> {
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

    pub fn general() -> Retained<NSPasteboard> {
        NSPasteboard::generalPasteboard()
    }
}

/// Puts these files on the clipboard, replacing what was there.
#[cfg(target_os = "macos")]
pub fn copy_files(paths: &[String]) -> bool {
    mac::write(&mac::general(), paths)
}

/// The files on the clipboard (from Patina, Finder or any other app).
#[cfg(target_os = "macos")]
pub fn pasted_files() -> Vec<String> {
    mac::read(&mac::general())
}

#[cfg(not(target_os = "macos"))]
pub fn copy_files(_paths: &[String]) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn pasted_files() -> Vec<String> {
    Vec::new()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::mac;
    use objc2_app_kit::NSPasteboard;

    #[test]
    fn files_round_trip_through_a_pasteboard() {
        // A private pasteboard, so the test never touches the real clipboard.
        let pb = NSPasteboard::pasteboardWithUniqueName();
        let paths = vec!["/tmp/one file.txt".to_string(), "/Applications".to_string()];
        assert!(mac::write(&pb, &paths));
        assert_eq!(mac::read(&pb), paths);
        assert!(mac::write(&pb, &["/tmp".to_string()]));
        assert_eq!(mac::read(&pb), ["/tmp"]);
    }
}
