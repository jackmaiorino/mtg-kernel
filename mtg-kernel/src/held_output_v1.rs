//! Held output: the stderr lines of work that must leave no trace yet.
//!
//! Work that runs ahead of the commit it depends on (the next iteration's
//! collection under `update_tail_overlap_v1`) must not be seen before that
//! commit, in its log any more than in its files. Its threads install a
//! holder ([`install_v1`]). While a holder is installed on a thread, every
//! line the crate writes to stderr through [`line_v1`] on that thread, and
//! the message of any panic on it, goes to the holder. The holder keeps the
//! lines in memory until it is released ([`HeldOutputV1::release_v1`]: they
//! are written to stderr in their order, and every later line at once) or
//! discarded ([`HeldOutputV1::discard_v1`]: neither they nor any later line
//! is written). A thread without a holder writes to stderr at once, exactly
//! as `eprintln!` does. Threads do not inherit a holder: code that spreads
//! held work over its own threads installs the holder on each
//! ([`current_v1`]).
//!
//! Panic messages reach the holder through a process panic hook, installed
//! with the first holder. It wraps the hook in place at that moment and calls
//! it for every panic on a thread without a holder, so those print exactly as
//! before. A held panic message has the standard hook's form (the thread, the
//! location and the message, and a backtrace when the environment asks for
//! one). What the process itself writes when it aborts (a stack overflow, a
//! failed allocation) is not held.

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, Once, PoisonError};

enum StateV1 {
    /// Lines kept, in their order, until the holder is released or discarded.
    Holding(Vec<String>),
    /// Released: every line is written at once.
    Released,
    /// Discarded: every line is dropped.
    Discarded,
}

/// The held output of one piece of work (module documentation).
pub(crate) struct HeldOutputV1 {
    state: Mutex<StateV1>,
    #[cfg(test)]
    recorder: Option<RecorderV1>,
}

/// Test-only: what a recorded holder does instead of writing to stderr.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HeldEventV1 {
    /// A line written (to stderr, without a recorder).
    Written(String),
    /// The holder was released while it held `held` lines.
    Released { held: usize },
    /// The holder was discarded while it held `held` lines.
    Discarded { held: usize },
}

/// Test-only: receives a recorded holder's events, on the thread that makes
/// them, while the holder's lock is held.
#[cfg(test)]
pub(crate) type RecorderV1 = Arc<dyn Fn(HeldEventV1) + Send + Sync>;

thread_local! {
    /// The holder installed on this thread, if any.
    static INSTALLED: RefCell<Option<Arc<HeldOutputV1>>> = const { RefCell::new(None) };
    /// Whether this thread holds a holder's lock: a panic there (stderr
    /// itself failing) goes to the standard hook instead of waiting for it.
    static LOCKED: Cell<bool> = const { Cell::new(false) };
}

/// Marks this thread as holding a holder's lock until it drops.
struct LockedV1;

impl LockedV1 {
    fn enter_v1() -> Self {
        LOCKED.with(|locked| locked.set(true));
        Self
    }
}

impl Drop for LockedV1 {
    fn drop(&mut self) {
        let _ = LOCKED.try_with(|locked| locked.set(false));
    }
}

impl HeldOutputV1 {
    /// A new holder, holding nothing yet.
    pub(crate) fn new_v1() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(StateV1::Holding(Vec::new())),
            #[cfg(test)]
            recorder: None,
        })
    }

    /// Test-only: a new holder whose writes and changes go to `recorder`
    /// instead of stderr.
    #[cfg(test)]
    pub(crate) fn recorded_v1(recorder: RecorderV1) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(StateV1::Holding(Vec::new())),
            recorder: Some(recorder),
        })
    }

    #[cfg(test)]
    fn record_v1(&self, event: HeldEventV1) -> bool {
        match &self.recorder {
            Some(recorder) => {
                recorder(event);
                true
            }
            None => false,
        }
    }

    fn write_v1(&self, line: &str) {
        #[cfg(test)]
        if self.record_v1(HeldEventV1::Written(line.to_owned())) {
            return;
        }
        eprintln!("{line}");
    }

    /// A panic's message, from the panic hook: as [`Self::line_v1`], except
    /// that a released holder writes it without ever panicking. A failed
    /// `eprintln!` there would panic inside the hook, which aborts the
    /// process instead of unwinding to the worker's `catch_unwind`; so a
    /// failed write is ignored, as the standard hook ignores it.
    fn panic_line_v1(&self, text: String) {
        self.with_state_v1(|state| match state {
            StateV1::Holding(lines) => lines.push(text),
            StateV1::Released => {
                #[cfg(test)]
                if self.record_v1(HeldEventV1::Written(text.clone())) {
                    return;
                }
                use std::io::Write;
                let _ = writeln!(std::io::stderr().lock(), "{text}");
            }
            StateV1::Discarded => {}
        });
    }

    /// Runs `f` on the state under its lock, marked for the panic hook.
    fn with_state_v1<R>(&self, f: impl FnOnce(&mut StateV1) -> R) -> R {
        let _locked = LockedV1::enter_v1();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut state)
    }

    /// One line of the held work: kept while holding, written once released,
    /// dropped once discarded.
    pub(crate) fn line_v1(&self, line: String) {
        self.with_state_v1(|state| match state {
            StateV1::Holding(lines) => lines.push(line),
            StateV1::Released => self.write_v1(&line),
            StateV1::Discarded => {}
        });
    }

    /// Writes every held line in its order; every later line is written at
    /// once. Nothing changes once the holder is released or discarded.
    pub(crate) fn release_v1(&self) {
        self.with_state_v1(|state| {
            let held = match state {
                StateV1::Holding(lines) => std::mem::take(lines),
                StateV1::Released | StateV1::Discarded => return,
            };
            *state = StateV1::Released;
            #[cfg(test)]
            self.record_v1(HeldEventV1::Released { held: held.len() });
            for line in &held {
                self.write_v1(line);
            }
        });
    }

    /// Drops every held line and every later one.
    pub(crate) fn discard_v1(&self) {
        self.with_state_v1(|state| {
            #[cfg(test)]
            if let StateV1::Holding(lines) = state {
                self.record_v1(HeldEventV1::Discarded { held: lines.len() });
            }
            *state = StateV1::Discarded;
        });
    }
}

/// A holder installed on this thread ([`install_v1`]); the thread's previous
/// holder, if any, is back once it drops. It restores this thread's slot, so
/// it is never sent to another thread.
#[must_use = "the holder is installed only until this guard drops"]
pub(crate) struct InstalledV1 {
    previous: Option<Arc<HeldOutputV1>>,
    _thread: PhantomData<*const ()>,
}

impl Drop for InstalledV1 {
    fn drop(&mut self) {
        let previous = self.previous.take();
        let _ = INSTALLED.try_with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                *slot = previous;
            }
        });
    }
}

/// Installs `holder` on this thread until the returned guard drops.
pub(crate) fn install_v1(holder: &Arc<HeldOutputV1>) -> InstalledV1 {
    install_panic_hook_v1();
    let previous = INSTALLED.with(|slot| slot.replace(Some(Arc::clone(holder))));
    InstalledV1 {
        previous,
        _thread: PhantomData,
    }
}

/// The holder installed on this thread, if any: code that spreads its work
/// over threads it starts installs it on each.
pub(crate) fn current_v1() -> Option<Arc<HeldOutputV1>> {
    INSTALLED
        .try_with(|slot| slot.try_borrow().ok().and_then(|held| held.clone()))
        .ok()
        .flatten()
}

/// Writes `line` to stderr, as `eprintln!` does, or to this thread's holder
/// when one is installed.
pub(crate) fn line_v1(line: String) {
    match current_v1() {
        Some(holder) => holder.line_v1(line),
        None => eprintln!("{line}"),
    }
}

/// Installs, once per process, the panic hook that sends the message of a
/// panic on a thread with a holder to that holder, and leaves every other
/// panic to the hook in place before it.
fn install_panic_hook_v1() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let standard = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let locked = LOCKED.try_with(Cell::get).unwrap_or(true);
            match current_v1().filter(|_| !locked) {
                Some(holder) => holder.panic_line_v1(panic_text_v1(info)),
                None => standard(info),
            }
        }));
    });
}

/// A panic's message in the standard hook's form.
fn panic_text_v1(info: &std::panic::PanicHookInfo<'_>) -> String {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("<unnamed>");
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("Box<dyn Any>");
    let mut text = match info.location() {
        Some(location) => format!("thread '{name}' panicked at {location}:\n{message}"),
        None => format!("thread '{name}' panicked:\n{message}"),
    };
    let backtrace = std::backtrace::Backtrace::capture();
    if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
        text.push_str(&format!("\nstack backtrace:\n{backtrace}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recorded holder and the events it records.
    fn recorded_holder_v1() -> (Arc<HeldOutputV1>, Arc<Mutex<Vec<HeldEventV1>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let holder = HeldOutputV1::recorded_v1(Arc::new(move |event: HeldEventV1| {
            sink.lock().unwrap().push(event);
        }));
        (holder, events)
    }

    fn written_v1(line: &str) -> HeldEventV1 {
        HeldEventV1::Written(line.to_owned())
    }

    /// Nothing is written while the holder holds; release writes the held
    /// lines in their order and every later line at once; a discarded holder
    /// writes nothing, held or later, and a release after it changes nothing.
    #[test]
    fn held_lines_are_written_in_order_on_release_and_never_on_discard() {
        let (released, events) = recorded_holder_v1();
        released.line_v1("first".into());
        released.line_v1("second".into());
        assert!(events.lock().unwrap().is_empty(), "written while held");
        released.release_v1();
        released.line_v1("third".into());
        released.release_v1();
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                HeldEventV1::Released { held: 2 },
                written_v1("first"),
                written_v1("second"),
                written_v1("third"),
            ]
        );
        // Discarded after its release: later lines are dropped.
        released.discard_v1();
        released.line_v1("fourth".into());
        assert_eq!(events.lock().unwrap().len(), 4);

        let (discarded, events) = recorded_holder_v1();
        discarded.line_v1("held".into());
        discarded.discard_v1();
        discarded.line_v1("later".into());
        discarded.release_v1();
        discarded.line_v1("after the release".into());
        assert_eq!(
            *events.lock().unwrap(),
            vec![HeldEventV1::Discarded { held: 1 }]
        );
    }

    /// A line goes to the holder of the thread that writes it, only while
    /// the holder is installed there; another thread, and the same thread
    /// once the guard drops, have no holder.
    #[test]
    fn a_thread_writes_through_its_installed_holder_only() {
        let (holder, events) = recorded_holder_v1();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                assert!(current_v1().is_none());
                let installed = install_v1(&holder);
                assert!(current_v1().is_some_and(|current| Arc::ptr_eq(&current, &holder)));
                line_v1("from the held thread".into());
                // A thread this one starts has no holder of its own.
                std::thread::scope(|inner| {
                    inner.spawn(|| assert!(current_v1().is_none()));
                });
                drop(installed);
                assert!(current_v1().is_none());
            });
        });
        assert!(events.lock().unwrap().is_empty());
        holder.release_v1();
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                HeldEventV1::Released { held: 1 },
                written_v1("from the held thread")
            ]
        );
    }

    /// The message of a panic on a thread with a holder goes to the holder
    /// in the standard form: held while it holds, written on release, never
    /// written when it is discarded.
    #[test]
    fn a_panic_on_a_holder_thread_is_held_with_its_message() {
        for release in [true, false] {
            let (holder, events) = recorded_holder_v1();
            std::thread::Builder::new()
                .name("held-panic".into())
                .spawn({
                    let holder = Arc::clone(&holder);
                    move || {
                        let _installed = install_v1(&holder);
                        let caught = std::panic::catch_unwind(|| panic!("a held panic"));
                        assert!(caught.is_err());
                    }
                })
                .unwrap()
                .join()
                .unwrap();
            assert!(events.lock().unwrap().is_empty(), "written while held");
            if release {
                holder.release_v1();
                let events = events.lock().unwrap();
                assert_eq!(events[0], HeldEventV1::Released { held: 1 });
                let HeldEventV1::Written(text) = &events[1] else {
                    panic!("{events:?}");
                };
                assert!(
                    text.starts_with("thread 'held-panic' panicked at ")
                        && text.contains("held_output_v1.rs")
                        && text.contains("\na held panic"),
                    "{text}"
                );
            } else {
                holder.discard_v1();
                assert_eq!(
                    *events.lock().unwrap(),
                    vec![HeldEventV1::Discarded { held: 1 }]
                );
            }
        }
    }
}
