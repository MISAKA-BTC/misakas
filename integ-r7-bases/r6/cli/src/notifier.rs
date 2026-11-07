//! Notifier provides a lightweight icon-based notification system. Icons show
//! up in the top right corner of the screen and disappear after a short delay.
//! Notification icons indicate clipboard copy and processing that may take
//! an extended period of time.

use crate::imports::*;
#[cfg(target_arch = "wasm32")]
use application_runtime::{is_nw, is_web};
#[cfg(target_arch = "wasm32")]
use web_sys::Element;
#[cfg(target_arch = "wasm32")]
use workflow_dom::{inject::inject_css, utils::*};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Notification {
    Transaction,
    Clipboard,
    Processing,
}

// `Inner` holds `web_sys::Element` DOM handles, which are affine to the JS
// runtime thread and are NOT inherently `Send`/`Sync`. On native targets those
// handles are NEVER instantiated (`is_nw()`/`is_web()` are compile-time `false`
// off-wasm, so `try_init` never calls `create_elements()`), so the native
// variant carries no `Element` fields at all and auto-derives `Send + Sync`
// with NO `unsafe`. Only the wasm variant needs the hand-written markers.
#[cfg(target_arch = "wasm32")]
struct Inner {
    elements: Mutex<Option<HashMap<Notification, Element>>>,
    current: Mutex<Option<Element>>,
}

#[cfg(not(target_arch = "wasm32"))]
struct Inner;

// SAFETY (wasm32 only): `Inner` stores `web_sys::Element` DOM handles, which are
// affine to the JavaScript runtime thread and are NOT inherently `Send`/`Sync`.
// On wasm32 the runtime is strictly single-threaded: every `Element` is created,
// read, and mutated on the one JS thread, so it is never actually moved across
// threads. The `Send`/`Sync` markers only exist to satisfy the `Arc`/async APIs
// (e.g. `workflow_core::task::spawn`) that require them. On native, `Inner` has
// no `Element` fields, so it auto-derives `Send + Sync` and no `unsafe` is used.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for Inner {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for Inner {}

#[derive(Clone)]
pub struct Notifier {
    // On native, `Inner` is a fieldless unit struct that is never read; it exists
    // only to keep `Notifier`'s shape identical across targets (and to provide
    // auto-derived `Send + Sync` with no `unsafe`).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    inner: Arc<Inner>,
}

// SAFETY (wasm32 only): `Notifier` is a thin `Arc<Inner>` wrapper; its
// thread-safety is entirely delegated to `Inner` above (see the soundness
// justification there). On native, `Inner: Send + Sync` is auto-derived, so
// `Arc<Inner>` and thus `Notifier` are `Send + Sync` with no `unsafe`.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for Notifier {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for Notifier {}

#[cfg(target_arch = "wasm32")]
impl Notifier {
    pub fn try_new() -> Result<Notifier> {
        Ok(Notifier { inner: Arc::new(Inner { elements: Mutex::new(None), current: Mutex::new(None) }) })
    }

    pub fn try_init(&self) -> Result<()> {
        let elements = if is_nw() || is_web() { Some(Self::create_elements()?) } else { None };
        *self.inner.elements.lock().unwrap() = elements;
        Ok(())
    }

    pub fn notify(&self, kind: Notification) {
        if let Some(elements) = self.inner.elements.lock().unwrap().as_ref()
            && let Some(el) = elements.get(&kind)
        {
            el.class_list().add_1("show").unwrap();
            let el = Sendable(el.clone());
            spawn(async move {
                yield_executor().await;
                sleep(Duration::from_millis(10)).await;
                el.class_list().remove_1("show").unwrap();
            })
        }
    }

    pub async fn notify_async(&self, kind: Notification) {
        self.notify(kind);
        yield_executor().await;
    }

    pub async fn show(&self, kind: Notification) -> NotifierGuard {
        // let mut inner = self.inner();
        if let Some(elements) = self.inner.elements.lock().unwrap().as_ref()
            && let Some(el) = elements.get(&kind)
        {
            el.class_list().add_1("show").unwrap();
            self.inner.current.lock().unwrap().replace(el.clone());
        }

        yield_executor().await;
        sleep(Duration::from_millis(10)).await;
        NotifierGuard::new(self)
    }

    pub async fn hide_async(&self) {
        if let Some(el) = self.inner.current.lock().unwrap().take() {
            el.class_list().remove_1("show").unwrap();
        }
    }

    pub fn hide(&self) {
        if let Some(el) = self.inner.current.lock().unwrap().take() {
            el.class_list().remove_1("show").unwrap();
        }
    }

    pub fn create_elements() -> Result<HashMap<Notification, Element>> {
        let mut elements = HashMap::new();

        inject_css(None, include_str!("./notifier.css"))?;

        let document = document();
        let body = body()?;

        let el = document.create_element("div").unwrap();
        el.set_class_name("notification transaction");
        body.append_child(&el).unwrap();
        elements.insert(Notification::Transaction, el);

        let el = document.create_element("div").unwrap();
        el.set_class_name("notification processing");
        body.append_child(&el).unwrap();
        elements.insert(Notification::Processing, el);

        let el = document.create_element("div").unwrap();
        el.set_class_name("notification clipboard");
        body.append_child(&el).unwrap();
        elements.insert(Notification::Clipboard, el);

        Ok(elements)
    }
}

// Native variant: there is no DOM, so the notifier is a no-op. `Inner` carries
// no `web_sys::Element` handles, so `Notifier` auto-derives `Send + Sync` with
// no `unsafe`. The public API mirrors the wasm32 impl above so callers in
// `KaspaCli` compile identically on both targets.
#[cfg(not(target_arch = "wasm32"))]
impl Notifier {
    pub fn try_new() -> Result<Notifier> {
        Ok(Notifier { inner: Arc::new(Inner) })
    }

    pub fn try_init(&self) -> Result<()> {
        Ok(())
    }

    pub fn notify(&self, _kind: Notification) {}

    pub async fn notify_async(&self, kind: Notification) {
        self.notify(kind);
        yield_executor().await;
    }

    pub async fn show(&self, _kind: Notification) -> NotifierGuard {
        yield_executor().await;
        sleep(Duration::from_millis(10)).await;
        NotifierGuard::new(self)
    }

    pub async fn hide_async(&self) {}

    pub fn hide(&self) {}
}

#[must_use = "if unused the notification will immediately disappear"]
#[clippy::has_significant_drop]
pub struct NotifierGuard {
    notifier: Notifier,
}

impl NotifierGuard {
    pub fn new(notifier: &Notifier) -> NotifierGuard {
        NotifierGuard { notifier: notifier.clone() }
    }

    pub fn hide(&self) {
        self.notifier.hide();
    }
}

impl Drop for NotifierGuard {
    fn drop(&mut self) {
        self.notifier.hide();
    }
}
