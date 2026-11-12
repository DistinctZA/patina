//! The system Quick Look panel, owned by a responder in Patina's window.
//! AppKit and every object here stay on the main thread.

use std::cell::RefCell;

use objc2::{
    DeclaredClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSResponder, NSWindow, NSWindowDelegate};
use objc2_foundation::{NSInteger, NSObjectProtocol, NSString, NSURL};
use objc2_quick_look_ui::{QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource, QLPreviewPanelDelegate};

struct PreviewState {
    items: RefCell<Vec<Retained<NSURL>>>,
    // NSResponder's nextResponder is unretained. Keep the original chain alive.
    next: Option<Retained<NSResponder>>,
}

define_class!(
    #[unsafe(super(NSResponder))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PreviewState]
    struct PatinaPreviewController;

    impl PatinaPreviewController {
        #[unsafe(method(acceptsPreviewPanelControl:))]
        fn accepts_preview_panel_control(&self, _panel: Option<&QLPreviewPanel>) -> bool {
            !self.ivars().items.borrow().is_empty()
        }

        #[unsafe(method(beginPreviewPanelControl:))]
        fn begin_preview_panel_control(&self, panel: &QLPreviewPanel) {
            // Quick Look calls this only after handing us control. The thread
            // local below retains us for the lifetime of the app's window.
            unsafe {
                panel.setDataSource(Some(ProtocolObject::from_ref(self)));
                panel.setDelegate(Some(self));
            }
        }

        #[unsafe(method(endPreviewPanelControl:))]
        fn end_preview_panel_control(&self, panel: &QLPreviewPanel) {
            unsafe {
                panel.setDelegate(None);
                panel.setDataSource(None);
            }
        }
    }

    unsafe impl NSObjectProtocol for PatinaPreviewController {}
    unsafe impl NSWindowDelegate for PatinaPreviewController {}

    unsafe impl QLPreviewPanelDataSource for PatinaPreviewController {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn number_of_preview_items(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            self.ivars().items.borrow().len() as NSInteger
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn preview_item(&self, _panel: Option<&QLPreviewPanel>, index: NSInteger)
            -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>>
        {
            usize::try_from(index).ok()
                .and_then(|index| self.ivars().items.borrow().get(index).cloned())
                .map(ProtocolObject::from_retained)
        }
    }

    unsafe impl QLPreviewPanelDelegate for PatinaPreviewController {
        #[unsafe(method(previewPanel:handleEvent:))]
        fn handle_event(&self, panel: Option<&QLPreviewPanel>, event: Option<&NSEvent>) -> bool {
            handle_key(panel, event)
        }
    }
);

fn handle_key(panel: Option<&QLPreviewPanel>, event: Option<&NSEvent>) -> bool {
    let (Some(panel), Some(event)) = (panel, event) else {
        return false;
    };
    if event.r#type() != NSEventType::KeyDown {
        return false;
    }
    let modifiers = NSEventModifierFlags::Command
        | NSEventModifierFlags::Control
        | NSEventModifierFlags::Option
        | NSEventModifierFlags::Shift;
    if event.modifierFlags().intersects(modifiers) {
        return false;
    }
    let key = event.charactersIgnoringModifiers().map(|s| s.to_string());
    if matches!(key.as_deref(), Some(" " | "\u{1b}")) {
        if !event.isARepeat() {
            panel.orderOut(None);
        }
        return true;
    }
    false
}

thread_local! {
    static CONTROLLER: RefCell<Option<Retained<PatinaPreviewController>>> = const { RefCell::new(None) };
}

/// Show the selected files using their real file URLs, including spaces and
/// Unicode names. A synchronous Tauri command calls this on the main thread.
pub fn show(window: &tauri::Window, paths: &[String], index: usize) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let mtm = MainThreadMarker::new().ok_or("Quick Look must open on the main thread")?;
    let raw_window = window.ns_window().map_err(|e| e.to_string())?;
    // Tauri owns this NSWindow for the duration of the command.
    let window = unsafe { &*raw_window.cast::<NSWindow>() };
    let controller = CONTROLLER.with(|slot| {
        let mut slot = slot.borrow_mut();
        slot.get_or_insert_with(|| {
            let next = unsafe { window.nextResponder() };
            let this = PatinaPreviewController::alloc(mtm).set_ivars(PreviewState {
                items: RefCell::new(Vec::new()),
                next,
            });
            let this: Retained<PatinaPreviewController> = unsafe { msg_send![super(this), init] };
            unsafe {
                this.setNextResponder(this.ivars().next.as_deref());
                window.setNextResponder(Some(&this));
            }
            this
        })
        .clone()
    });
    *controller.ivars().items.borrow_mut() = paths
        .iter()
        .map(|path| NSURL::fileURLWithPath(&NSString::from_str(path)))
        .collect();
    // Drop all RefCell borrows before calling AppKit: reloadData calls our
    // data source synchronously, and must be able to borrow the items again.
    let panel = unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }.ok_or("Couldn't open Quick Look")?;
    unsafe {
        panel.updateController();
        panel.reloadData();
        panel.setCurrentPreviewItemIndex(index.min(paths.len() - 1) as NSInteger);
    }
    panel.makeKeyAndOrderFront(None);
    Ok(())
}
