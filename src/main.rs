//! OTP Bar: a menu bar key icon. Click it, paste a secret key, and the
//! current access code is on your clipboard before you let go of ⌘V.

mod totp;

use std::cell::{Cell, OnceCell, RefCell};
use std::time::{SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSButton, NSColor,
    NSControlTextEditingDelegate, NSFont, NSImage, NSMenu, NSMenuItem, NSPasteboard,
    NSPasteboardTypeString, NSPopover, NSPopoverBehavior, NSStatusBar, NSStatusItem,
    NSTextField, NSTextFieldDelegate, NSView, NSViewController, NSVariableStatusItemLength,
};
use objc2_foundation::{NSNotification, NSObjectProtocol, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSTimer};

use totp::Secret;

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// "123456" -> "123 456" for reading; the clipboard always gets the plain digits.
fn pretty(code: &str) -> String {
    let mid = code.len() / 2;
    format!("{} {}", &code[..mid], &code[mid..])
}

#[derive(Default)]
struct Ivars {
    status_item: OnceCell<Retained<NSStatusItem>>,
    popover: OnceCell<Retained<NSPopover>>,
    input: OnceCell<Retained<NSTextField>>,
    code_button: OnceCell<Retained<NSButton>>,
    status: OnceCell<Retained<NSTextField>>,
    /// Held in memory only (never written to disk) so a fresh code is one click away.
    secret: RefCell<Option<Secret>>,
    /// Counter of the code that is currently on the clipboard.
    copied_counter: Cell<Option<u64>>,
    last_len: Cell<usize>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "OtpBarDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _n: &NSNotification) {
            self.build_ui();
        }
    }

    unsafe impl NSControlTextEditingDelegate for Delegate {}
    unsafe impl NSTextFieldDelegate for Delegate {}

    impl Delegate {
        /// Fires on every edit. A jump of several characters at once is a paste:
        /// act on it immediately. Typing by hand waits for Return.
        #[unsafe(method(controlTextDidChange:))]
        fn text_did_change(&self, _n: &NSNotification) {
            let text = self.input().stringValue().to_string();
            let grew = text.len().saturating_sub(self.ivars().last_len.get());
            self.ivars().last_len.set(text.len());
            if grew >= 8 {
                self.take_secret(&text);
            }
        }

        #[unsafe(method(submit:))]
        fn submit(&self, _sender: Option<&AnyObject>) {
            let text = self.input().stringValue().to_string();
            if !text.trim().is_empty() {
                self.take_secret(&text);
            }
        }

        #[unsafe(method(togglePopover:))]
        fn toggle_popover(&self, _sender: Option<&AnyObject>) {
            let popover = self.ivars().popover.get().unwrap();
            if popover.isShown() {
                popover.close();
                return;
            }
            let mtm = self.mtm();
            let item = self.ivars().status_item.get().unwrap();
            let Some(button) = item.button(mtm) else { return };
            NSApplication::sharedApplication(mtm).activate();
            popover.showRelativeToRect_ofView_preferredEdge(button.bounds(), &button, NSRectEdge::MinY);
            if let Some(window) = self.input().window() {
                window.makeFirstResponder(Some(self.input()));
            }
            self.refresh();
        }

        #[unsafe(method(copyCode:))]
        fn copy_code(&self, _sender: Option<&AnyObject>) {
            let secret = self.ivars().secret.borrow().clone();
            if let Some(secret) = secret {
                let t = now();
                copy_to_clipboard(&secret.code_at(t));
                self.ivars().copied_counter.set(Some(secret.counter(t)));
                self.refresh();
            }
        }

        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            if self.ivars().popover.get().is_some_and(|p| p.isShown()) {
                self.refresh();
            }
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        unsafe { msg_send![super(this), init] }
    }

    fn input(&self) -> &NSTextField {
        self.ivars().input.get().unwrap()
    }

    fn target(&self) -> &AnyObject {
        self.as_ref()
    }

    fn take_secret(&self, text: &str) {
        match Secret::parse(text) {
            Ok(secret) => {
                let t = now();
                copy_to_clipboard(&secret.code_at(t));
                self.ivars().copied_counter.set(Some(secret.counter(t)));
                *self.ivars().secret.borrow_mut() = Some(secret);
                // Don't leave the secret sitting in the text field.
                self.input().setStringValue(&ns(""));
                self.ivars().last_len.set(0);
                self.refresh();
            }
            Err(e) => self.set_status(&e, true),
        }
    }

    fn refresh(&self) {
        let button = self.ivars().code_button.get().unwrap();
        let secret = self.ivars().secret.borrow();
        let Some(secret) = secret.as_ref() else {
            button.setTitle(&ns("––– –––"));
            self.set_status("Paste a secret key (⌘V)", false);
            return;
        };
        let t = now();
        button.setTitle(&ns(&pretty(&secret.code_at(t))));
        let left = secret.seconds_left(t);
        if self.ivars().copied_counter.get() == Some(secret.counter(t)) {
            self.set_status(&format!("✓ Copied to clipboard · {left}s left"), false);
        } else {
            self.set_status(&format!("New code · click it to copy · {left}s left"), false);
        }
    }

    fn set_status(&self, text: &str, error: bool) {
        let status = self.ivars().status.get().unwrap();
        status.setStringValue(&ns(text));
        let color = if error { NSColor::systemRedColor() } else { NSColor::secondaryLabelColor() };
        status.setTextColor(Some(&color));
    }

    fn build_ui(&self) {
        let mtm = self.mtm();
        let app = NSApplication::sharedApplication(mtm);
        app.setMainMenu(Some(&edit_menu(mtm)));

        // Menu bar icon.
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &ns("key.fill"),
                Some(&ns("OTP Bar")),
            );
            match image {
                Some(image) => {
                    image.setTemplate(true);
                    button.setImage(Some(&image));
                }
                None => button.setTitle(&ns("OTP")),
            }
            unsafe {
                button.setTarget(Some(self.target()));
                button.setAction(Some(sel!(togglePopover:)));
            }
        }

        // Popover contents.
        let (w, h) = (300.0, 150.0);
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, w, h));

        let input = NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(16.0, 106.0, w - 32.0, 26.0));
        input.setPlaceholderString(Some(&ns("Paste secret key or otpauth:// link")));
        unsafe { input.setDelegate(Some(ProtocolObject::from_ref(self))) };
        unsafe {
            input.setTarget(Some(self.target()));
            input.setAction(Some(sel!(submit:)));
        }
        view.addSubview(&input);

        let code = unsafe {
            NSButton::buttonWithTitle_target_action(&ns("––– –––"), Some(self.target()), Some(sel!(copyCode:)), mtm)
        };
        code.setBordered(false);
        code.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(30.0, 0.3)));
        code.setFrame(rect(16.0, 46.0, w - 32.0, 50.0));
        code.setToolTip(Some(&ns("Click to copy the current code")));
        view.addSubview(&code);

        let status = NSTextField::labelWithString(&ns(""), mtm);
        status.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        status.setFrame(rect(16.0, 16.0, w - 80.0, 18.0));
        view.addSubview(&status);

        let quit = unsafe { NSButton::buttonWithTitle_target_action(&ns("Quit"), Some(self.target()), Some(sel!(quit:)), mtm) };
        quit.setBordered(false);
        quit.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        quit.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        quit.setFrame(rect(w - 56.0, 14.0, 40.0, 20.0));
        view.addSubview(&quit);

        let vc = NSViewController::new(mtm);
        vc.setView(&view);
        let popover = NSPopover::new(mtm);
        popover.setContentViewController(Some(&vc));
        popover.setContentSize(NSSize::new(w, h));
        popover.setBehavior(NSPopoverBehavior::Transient);
        popover.setAnimates(false);

        let ivars = self.ivars();
        let _ = ivars.status_item.set(item);
        let _ = ivars.popover.set(popover);
        let _ = ivars.input.set(input);
        let _ = ivars.code_button.set(code);
        let _ = ivars.status.set(status);

        unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                1.0,
                self.target(),
                sel!(tick:),
                None,
                true,
            );
        }
        self.refresh();
    }
}

/// Accessory apps have no visible menu bar, but ⌘V/⌘C/⌘A in a text field only
/// work if an Edit menu with those key equivalents exists.
fn edit_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let item = |title: &str, action, key: &str| unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &ns(title), Some(action), &ns(key))
    };
    let main = NSMenu::new(mtm);
    let app_menu = NSMenu::new(mtm);
    app_menu.addItem(&item("Quit OTP Bar", sel!(terminate:), "q"));
    let edit = NSMenu::initWithTitle(NSMenu::alloc(mtm), &ns("Edit"));
    edit.addItem(&item("Cut", sel!(cut:), "x"));
    edit.addItem(&item("Copy", sel!(copy:), "c"));
    edit.addItem(&item("Paste", sel!(paste:), "v"));
    edit.addItem(&item("Select All", sel!(selectAll:), "a"));
    edit.addItem(&item("Undo", sel!(undo:), "z"));
    for sub in [app_menu, edit] {
        let holder = NSMenuItem::new(mtm);
        holder.setSubmenu(Some(&sub));
        main.addItem(&holder);
    }
    main
}

fn copy_to_clipboard(code: &str) {
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    pb.setString_forType(&ns(code), unsafe { NSPasteboardTypeString });
    // nspasteboard.org convention: clipboard managers skip concealed items.
    pb.setString_forType(&ns(""), &ns("org.nspasteboard.ConcealedType"));
}

fn main() {
    // CLI mode for scripts and testing: `otp-bar --code <secret>`.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--code") {
        let input = args[2..].join(" ");
        match Secret::parse(&input) {
            Ok(s) => println!("{}", s.code_at(now())),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let mtm = MainThreadMarker::new().expect("must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = Delegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
