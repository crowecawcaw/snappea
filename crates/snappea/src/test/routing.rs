//! Routes events from the global [`snappea::emit::emit`](crate::emit::emit) into in-process recordings.
//!
//! A recording binds the thread that creates it (the test's thread); [`Recording::bind`]
//! binds others. An emit on a bound thread goes to that thread's newest open recording.
//! On an unbound thread it goes to the only open recording, or, when several are open
//! (Cargo runs tests in parallel), fails all of them: the event cannot be attributed.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, Weak};

use crate::emit::Event;

use crate::test::{Buffer, ErrorKind, Recording};

type Shared = Weak<Mutex<Buffer>>;

static OPEN: Mutex<Vec<Shared>> = Mutex::new(Vec::new());

thread_local! {
    static BOUND: RefCell<Vec<Shared>> = const { RefCell::new(Vec::new()) };
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Registers an open recording and binds it to the current thread.
pub(crate) fn open(state: &Arc<Mutex<Buffer>>) {
    let mut open = lock(&OPEN);
    open.retain(|w| w.strong_count() > 0);
    open.push(Arc::downgrade(state));
    crate::emit::test_hook::set_routing(route, true);
    drop(open);
    bind(state);
}

/// Unregisters a recording (at its check or drop) and unbinds it from this thread.
pub(crate) fn close(state: &Arc<Mutex<Buffer>>) {
    let mut open = lock(&OPEN);
    open.retain(|w| w.strong_count() > 0 && !std::ptr::eq(w.as_ptr(), Arc::as_ptr(state)));
    crate::emit::test_hook::set_routing(route, !open.is_empty());
    drop(open);
    unbind(state);
}

fn bind(state: &Arc<Mutex<Buffer>>) {
    BOUND.with(|b| b.borrow_mut().push(Arc::downgrade(state)));
}

fn unbind(state: &Arc<Mutex<Buffer>>) {
    BOUND.with(|b| {
        let mut b = b.borrow_mut();
        if let Some(i) = b.iter().rposition(|w| std::ptr::eq(w.as_ptr(), Arc::as_ptr(state))) {
            b.remove(i);
        }
    });
}

/// Keeps a thread bound to a recording until dropped. See [`Recording::bind`].
#[must_use = "the thread is bound only while the guard lives"]
pub struct BindGuard {
    state: Arc<Mutex<Buffer>>,
    _not_send: PhantomData<*const ()>,
}

impl Drop for BindGuard {
    fn drop(&mut self) {
        unbind(&self.state);
    }
}

impl Recording {
    /// Sends the global [`snappea::emit::emit`](crate::emit::emit) calls made on this
    /// thread to this recording until the guard drops. The test's own thread is bound
    /// already; bind the scoped threads it spawns. A `'static` thread (`thread::spawn`)
    /// cannot borrow the recording: use a scoped thread, or inject
    /// [`emitter`](Self::emitter) into the code it runs.
    ///
    /// ```ignore
    /// std::thread::scope(|s| {
    ///     s.spawn(|| {
    ///         let _guard = recording.bind();
    ///         app.reserve("coffee", "o1", 2); // calls snappea::emit::emit
    ///     });
    /// });
    /// ```
    pub fn bind(&self) -> BindGuard {
        bind(&self.state);
        BindGuard { state: self.state.clone(), _not_send: PhantomData }
    }
}

fn is_open(state: &Mutex<Buffer>) -> bool {
    !lock(state).closed
}

fn route(event: Result<Event, String>) -> Option<Event> {
    let bound = BOUND.with(|b| {
        let mut b = b.borrow_mut();
        b.retain(|w| w.strong_count() > 0);
        b.iter().rev().filter_map(Weak::upgrade).find(|s| is_open(s))
    });
    let target = match bound {
        Some(state) => state,
        None => {
            let open: Vec<_> = lock(&OPEN).iter().filter_map(Weak::upgrade).filter(|s| is_open(s)).collect();
            match open.as_slice() {
                [] => return event.ok(),
                [one] => one.clone(),
                several => {
                    let what = match &event {
                        Ok(e) => format!("{} ({})", e.event, e.src.as_deref().unwrap_or("?")),
                        Err(message) => message.clone(),
                    };
                    let message = format!(
                        "snappea::emit::emit of {what} ran on a thread not bound to a recording while {} recordings \
                         were open, so it could not be attributed. Bind the thread \
                         (`let _guard = recording.bind();` in a scoped thread) or pass \
                         `recording.emitter()` to the code that emits.",
                        several.len()
                    );
                    for state in several {
                        lock(state).errors.push((ErrorKind::Config, message.clone()));
                    }
                    return None;
                }
            }
        }
    };
    let mut state = lock(&target);
    match event {
        Ok(event) => state.events.push(event),
        Err(message) => state.errors.push((ErrorKind::Parser, message)),
    }
    None
}
