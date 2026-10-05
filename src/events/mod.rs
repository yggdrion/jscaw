//! Shared plumbing for `on*` subscriptions: a per-JS-thread registry of live COM
//! registrations, each unregistered when its JS `unsubscribe()` runs or the env tears down.

pub mod device;

use crate::com::ComGuard;
use napi::bindgen_prelude::Function;
use napi::{Env, Result};
use std::cell::RefCell;
use std::collections::HashMap;

/// One live registration. `unregister` runs on drop, before `_com` releases the thread's COM
/// init, and drops whatever it captured (COM objects, the threadsafe function) with it.
pub struct Subscription {
    unregister: Option<Box<dyn FnOnce()>>,
    _com: ComGuard,
}

impl Subscription {
    pub fn new(com: ComGuard, unregister: impl FnOnce() + 'static) -> Self {
        Self {
            unregister: Some(Box::new(unregister)),
            _com: com,
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(unregister) = self.unregister.take() {
            unregister();
        }
    }
}

#[derive(Default)]
struct Registry {
    next_id: u32,
    subs: HashMap<u32, Subscription>,
    cleanup_hooked: bool,
}

thread_local! {
    // Each JS thread (main or Worker) has its own env, so its own registry.
    static REGISTRY: RefCell<Registry> = RefCell::default();
}

fn remove(id: u32) {
    // Taken out first so the drop (which calls into COM) runs without the borrow held.
    let sub = REGISTRY.with(|r| r.borrow_mut().subs.remove(&id));
    drop(sub);
}

fn clear() {
    let subs = REGISTRY.with(|r| std::mem::take(&mut r.borrow_mut().subs));
    drop(subs);
}

/// Stores `sub` and returns its idempotent JS `unsubscribe` function.
pub fn subscribe(env: &Env, sub: Subscription) -> Result<Function<'_, (), ()>> {
    let hook_needed = REGISTRY.with(|r| !r.borrow().cleanup_hooked);
    if hook_needed {
        env.add_env_cleanup_hook((), |_| clear())?;
    }
    let id = REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        r.cleanup_hooked = true;
        r.next_id += 1;
        let id = r.next_id;
        r.subs.insert(id, sub);
        id
    });
    env.create_function_from_closure("unsubscribe", move |_| {
        remove(id);
        Ok(())
    })
}
