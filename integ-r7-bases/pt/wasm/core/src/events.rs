use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Sink {
    context: Option<Object>,
    callback: Function,
}

impl Sink {
    pub fn new<F>(callback: F) -> Self
    where
        F: AsRef<Function>,
    {
        Self { context: None, callback: callback.as_ref().clone() }
    }

    pub fn with_context(mut self, context: Option<Object>) -> Self {
        self.context = context;
        self
    }

    pub fn call(&self, args: &JsValue) -> std::result::Result<JsValue, JsValue> {
        if let Some(context) = &self.context {
            self.callback.call1(context, args)
        } else {
            self.callback.call1(&JsValue::UNDEFINED, args)
        }
    }
}

// SAFETY: `Sink` wraps `js_sys::Function`/`js_sys::Object`, which are handles
// affine to the JavaScript runtime thread and are NOT inherently `Send`.
// Asserting `Send` is only sound under WASM, where the runtime is strictly
// single-threaded: all `Sink`s are created, stored, and invoked on the one and
// only JS thread, so the handle is never actually moved across threads. The
// `Send` bound merely satisfies the `async`/executor APIs that require it.
//
// On native (multithreaded) targets this assertion would be UNSOUND, so it is
// gated out. `Sink` is only ever constructed/used from `#[cfg(target_arch =
// "wasm32")]` / wasm32-feature-gated code (kaspa-wallet-core, kaspa-wrpc-wasm),
// so native builds never need `Sink: Send`.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for Sink {}

impl Sink {
    pub fn try_from<T>(value: T) -> std::result::Result<Self, JsValue>
    where
        T: AsRef<JsValue>,
    {
        let value = value.as_ref();
        if let Some(callback) = value.dyn_ref::<Function>() {
            Ok(Sink::new(callback))
        } else if let Some(context) = value.dyn_ref::<Object>() {
            let callback = Reflect::get(context, &JsValue::from("handleEvent"))
                .map_err(|_| JsValue::from("Object does not have 'handleEvent()' method"))?
                .dyn_into::<Function>()
                .map_err(|_| JsValue::from("'handleEvent()' is not a function"))?;
            Ok(Sink::new(callback).with_context(Some(context.clone())))
        } else {
            Err(JsValue::from(format!("Invalid event listener callback: {:?}", value)))
        }
    }
}

pub fn get_event_targets<T, R, E>(targets: T) -> std::result::Result<Vec<R>, E>
where
    T: Into<JsValue>,
    R: TryFrom<JsValue, Error = E>,
{
    let js_value = targets.into();
    if let Ok(array) = js_value.clone().dyn_into::<Array>() {
        array.iter().map(|item| R::try_from(item)).collect()
    } else {
        Ok(vec![R::try_from(js_value)?])
    }
}
