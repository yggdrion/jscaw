//! Shared plumbing for `on*` subscriptions: a per-JS-thread registry of live COM
//! registrations, each unregistered when its JS `unsubscribe()` runs or the env tears down.

pub mod device;
pub mod endpoint;

use crate::com::ComGuard;
use napi::bindgen_prelude::{Function, JsValuesTupleIntoVec, Unknown};
use napi::threadsafe_function::ThreadsafeFunction;
use napi::{Env, Result};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

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
    let subs = REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        // The env is gone; a later env on this thread (e.g. an Electron reload) needs its own hook.
        r.cleanup_hooked = false;
        std::mem::take(&mut r.subs)
    });
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

/// Queues events of type `T` from any thread to the JS thread.
pub type EventTsfn<T> = ThreadsafeFunction<T, (), T, napi::Status, false>;

/// Wraps `callback` in a threadsafe function. Events already queued when `unsubscribe()` runs
/// are still dispatched by Node, so it targets a gate that drops them once the returned flag
/// is cleared; the `unregister` closure clears it.
pub fn gated_tsfn<T: JsValuesTupleIntoVec + 'static>(
    env: &Env,
    name: &str,
    callback: Function<Unknown<'static>, ()>,
) -> Result<(EventTsfn<T>, Rc<Cell<bool>>)> {
    let active = Rc::new(Cell::new(true));
    let gate_active = active.clone();
    let user = callback.create_ref()?;
    let gate = env.create_function_from_closure::<T, (), _>(name, move |ctx| {
        if gate_active.get() {
            user.borrow_back(ctx.env)?
                .call(ctx.get::<Unknown<'static>>(0)?)?;
        }
        Ok(())
    })?;
    Ok((gate.build_threadsafe_function().build()?, active))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_rearms_the_cleanup_hook_for_the_next_env() {
        REGISTRY.with(|r| r.borrow_mut().cleanup_hooked = true);
        clear();
        assert!(!REGISTRY.with(|r| r.borrow().cleanup_hooked));
    }
}
