//! Who gets a preview made first.
//!
//! Making previews is the only slow thing this app does, and far more of them
//! are asked for than can be made at once: scroll a grid and every cell that
//! passes wants one. What keeps that feeling quick is not making them faster
//! but making them in the right order.
//!
//! Three rules:
//!
//! 1. **The photo being looked at goes first.** One worker takes nothing else,
//!    so the loupe never waits behind a queue of thumbnails.
//! 2. **Newest request first.** When a scroll stops, the cells on screen are
//!    the ones that asked last. Serving in arrival order would spend the disk
//!    on the part of the shoot that scrolled past on the way here.
//! 3. **Each preview is made once.** A second request for one already queued
//!    or under way waits for the first instead of starting again — which is
//!    how looking ahead and then actually arriving cost one decode, not two.
//! 4. **Getting ahead never gets in the way.** Preparing a whole folder in
//!    advance is what makes any photo open at once, but it is work nobody has
//!    asked for yet. It stops the moment anything is asked for, stays stopped
//!    for a beat afterwards, and never takes more than half the workers.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long idle work stays stopped after anything else is asked for.
///
/// Requests come in runs — a held arrow key, a scroll — with gaps between
/// them shorter than this. Without the pause, idle work would start up in
/// every gap and be mid-decode when the next request landed.
const QUIET: Duration = Duration::from_millis(400);

/// What everyone waiting on work abandoned before it started is told.
pub const ABANDONED: &str = "no longer wanted";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// For the photo on screen in the loupe.
    Now = 0,
    /// For a cell that is on screen.
    Soon = 1,
    /// Looking ahead: wanted shortly, not yet.
    Later = 2,
    /// Not wanted by anyone yet. Made only while nothing else is being asked
    /// for, and only on some of the workers.
    Idle = 3,
}

type Work<T> = Box<dyn FnOnce() -> Result<T, String> + Send>;
type Done<T> = Box<dyn FnOnce(Result<Arc<T>, String>) + Send>;

struct Job<T> {
    key: String,
    priority: Priority,
    /// Order of asking; larger is more recent.
    seq: u64,
    /// Someone has stopped to wait for this (`submit_waited`). Spare-time
    /// work like that is not abandoned when idle work is cleared, and goes
    /// before spare-time work nobody is waiting on.
    waited: bool,
    work: Work<T>,
}

struct State<T> {
    queue: Vec<Job<T>>,
    /// Everyone waiting on each key, whether it is queued or being made.
    waiting: HashMap<String, Vec<Done<T>>>,
    seq: u64,
    /// Idle work may not start before this.
    quiet_until: Instant,
    idle_running: usize,
    idle_limit: usize,
}

pub struct Pool<T> {
    shared: Arc<(Mutex<State<T>>, Condvar)>,
}

impl<T> Clone for Pool<T> {
    fn clone(&self) -> Self {
        Self { shared: self.shared.clone() }
    }
}

impl<T: Send + Sync + 'static> Pool<T> {
    /// `workers` threads in all, the first of which serves only `Now`. Idle
    /// work gets half of the rest.
    pub fn new(workers: usize) -> Self {
        Self::with_idle_limit(workers, (workers.max(2) - 1) / 2)
    }

    /// As `new`, with a stated number of workers that may do idle work at once.
    pub fn with_idle_limit(workers: usize, idle_limit: usize) -> Self {
        let workers = workers.max(2);
        let shared = Arc::new((
            Mutex::new(State {
                queue: Vec::new(),
                waiting: HashMap::new(),
                seq: 0,
                quiet_until: Instant::now(),
                idle_running: 0,
                idle_limit: idle_limit.clamp(1, workers - 1),
            }),
            Condvar::new(),
        ));
        for i in 0..workers {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name(format!("opencull-preview-{i}"))
                .spawn(move || Self::work(shared, i == 0))
                .expect("spawn preview worker");
        }
        Self { shared }
    }

    /// Asks for `key` to be made by `work`, and for `done` to be told.
    ///
    /// If `key` is already wanted, `work` is dropped unused and `done` joins
    /// the others waiting. Asking again also moves the job forward: to the
    /// front of the queue, and up to this request's priority if that is
    /// higher — so a photo that was only being looked ahead to jumps the
    /// queue the moment it is the one on screen. The exception is spare-time
    /// work asking for a job queued at a higher priority: that leaves the job
    /// where it was, so it never reorders what the photographer asked for.
    pub fn submit(
        &self,
        key: String,
        priority: Priority,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
        done: impl FnOnce(Result<Arc<T>, String>) + Send + 'static,
    ) {
        self.enqueue(key, priority, false, Box::new(work), Box::new(done));
    }

    /// As `submit`, for an asker that stops and waits for the answer. Such a
    /// request for spare-time work is kept when idle work is cleared, and
    /// goes before the spare-time work nobody is waiting on, so whoever is
    /// waiting is not left behind a fresh batch of preparation.
    pub fn submit_waited(
        &self,
        key: String,
        priority: Priority,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
        done: impl FnOnce(Result<Arc<T>, String>) + Send + 'static,
    ) {
        self.enqueue(key, priority, true, Box::new(work), Box::new(done));
    }

    fn enqueue(&self, key: String, priority: Priority, waited: bool, work: Work<T>, done: Done<T>) {
        let (lock, wake) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.seq += 1;
        let seq = state.seq;
        if priority != Priority::Idle {
            state.quiet_until = Instant::now() + QUIET;
        }

        if let Some(waiters) = state.waiting.get_mut(&key) {
            waiters.push(done);
            if let Some(job) = state.queue.iter_mut().find(|j| j.key == key) {
                job.waited |= waited;
                // Spare-time asking never reorders anything else.
                if priority != Priority::Idle || job.priority == Priority::Idle {
                    job.priority = job.priority.min(priority);
                    job.seq = seq;
                }
            }
        } else {
            state.waiting.insert(key.clone(), vec![done]);
            state.queue.push(Job { key, priority, seq, waited, work });
        }
        drop(state);
        wake.notify_all();
    }

    /// Abandons everything not yet started, telling whoever was waiting.
    /// For when a folder is closed: its thumbnails are no longer wanted, and
    /// should not hold up the next folder's.
    pub fn clear_queued(&self) {
        let (lock, _) = &*self.shared;
        let abandoned: Vec<Done<T>> = {
            let mut state = lock.lock().unwrap();
            let keys: Vec<String> = state.queue.drain(..).map(|j| j.key).collect();
            keys.iter().filter_map(|k| state.waiting.remove(k)).flatten().collect()
        };
        for done in abandoned {
            done(Err(ABANDONED.into()));
        }
    }

    /// Abandons idle work not yet started, and nothing else. For when what
    /// is worth preparing has changed — the photographer has moved to another
    /// part of the folder — and the old list would only delay the new one.
    /// Idle work someone has stopped to wait for is kept: what they wait on
    /// has not changed.
    pub fn clear_idle(&self) {
        let (lock, _) = &*self.shared;
        let abandoned: Vec<Done<T>> = {
            let mut state = lock.lock().unwrap();
            let mut keys = Vec::new();
            state.queue.retain(|j| {
                let idle = j.priority == Priority::Idle && !j.waited;
                if idle {
                    keys.push(j.key.clone());
                }
                !idle
            });
            keys.iter().filter_map(|k| state.waiting.remove(k)).flatten().collect()
        };
        for done in abandoned {
            done(Err(ABANDONED.into()));
        }
    }

    /// Whether the photographer is being served right now: something other
    /// than spare-time work is waiting, or was asked for a moment ago. Work
    /// that should never get in their way checks this and stands aside.
    pub fn busy(&self) -> bool {
        let state = self.shared.0.lock().unwrap();
        Instant::now() < state.quiet_until || state.queue.iter().any(|j| j.priority != Priority::Idle)
    }

    pub fn queued(&self) -> usize {
        self.shared.0.lock().unwrap().queue.len()
    }

    /// Idle work still to do: waiting, plus under way.
    pub fn idle_remaining(&self) -> usize {
        let state = self.shared.0.lock().unwrap();
        state.queue.iter().filter(|j| j.priority == Priority::Idle).count() + state.idle_running
    }

    fn work(shared: Arc<(Mutex<State<T>>, Condvar)>, only_now: bool) {
        let (lock, wake) = &*shared;
        loop {
            let job = {
                let mut state = lock.lock().unwrap();
                loop {
                    let now = Instant::now();
                    let idle_has_room = !only_now && state.idle_running < state.idle_limit;
                    let idle_allowed = idle_has_room && now >= state.quiet_until;
                    let pick = state
                        .queue
                        .iter()
                        .enumerate()
                        .filter(|(_, j)| match j.priority {
                            Priority::Now => true,
                            Priority::Idle => idle_allowed,
                            _ => !only_now,
                        })
                        // Highest priority, then most recently asked for —
                        // except that idle work someone is waiting on goes
                        // before idle work nobody is.
                        .min_by_key(|(_, j)| {
                            (j.priority, j.priority == Priority::Idle && !j.waited, std::cmp::Reverse(j.seq))
                        })
                        .map(|(i, _)| i);
                    if let Some(i) = pick {
                        let job = state.queue.swap_remove(i);
                        if job.priority == Priority::Idle {
                            state.idle_running += 1;
                        }
                        break job;
                    }
                    // Nothing to take. If idle work is only waiting out the
                    // quiet after a request, wake when that ends; nothing
                    // else is going to announce it.
                    let idle_waiting = idle_has_room && state.queue.iter().any(|j| j.priority == Priority::Idle);
                    state = if idle_waiting {
                        let pause = state.quiet_until.saturating_duration_since(now).max(Duration::from_millis(1));
                        wake.wait_timeout(state, pause).unwrap().0
                    } else {
                        wake.wait(state).unwrap()
                    };
                }
            };
            let was_idle = job.priority == Priority::Idle;

            // A decoder handed a corrupt file must not take the worker down
            // with it; the file fails and the thread carries on.
            let result = catch_unwind(AssertUnwindSafe(job.work))
                .unwrap_or_else(|_| Err("decoder panicked".into()))
                .map(Arc::new);

            let waiters = {
                let mut state = lock.lock().unwrap();
                if was_idle {
                    state.idle_running -= 1;
                }
                state.waiting.remove(&job.key).unwrap_or_default()
            };
            if was_idle {
                // A worker held back by the limit on idle work may now go.
                wake.notify_all();
            }
            for done in waiters {
                done(result.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    /// Holds every worker until released, so a queue can be built up behind
    /// them and the order it drains in observed.
    ///
    /// Two gates rather than one, because the two kinds of worker are not
    /// interchangeable: the reserved one takes only `Now`. Returns
    /// `(general, reserved)`; a test that wants an exact order releases the
    /// general workers alone, leaving a single thread to do everything.
    fn hold(pool: &Pool<u32>, workers: usize) -> (mpsc::Sender<()>, mpsc::Sender<()>) {
        let (open_general, general) = mpsc::channel::<()>();
        let (open_reserved, reserved) = mpsc::channel::<()>();
        let general = Arc::new(Mutex::new(general));
        let reserved = Arc::new(Mutex::new(reserved));
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        for i in 0..workers {
            let (general, reserved) = (general.clone(), reserved.clone());
            let ready = ready_tx.clone();
            // `Now`, so the reserved worker is held as well.
            pool.submit(
                format!("hold-{i}"),
                Priority::Now,
                move || {
                    let is_reserved = std::thread::current().name() == Some("opencull-preview-0");
                    ready.send(()).ok();
                    let gate = if is_reserved { reserved } else { general };
                    // Taken and dropped at once, so several general workers
                    // can each be let go by one send apiece.
                    let waiting = gate.lock().unwrap().recv();
                    waiting.ok();
                    Ok(0)
                },
                |_| {},
            );
        }
        for _ in 0..workers {
            ready_rx.recv_timeout(Duration::from_secs(5)).expect("workers should all start");
        }
        (open_general, open_reserved)
    }

    #[test]
    fn everything_asked_for_is_made_and_reported() {
        let pool = Pool::<u32>::new(4);
        let (tx, rx) = mpsc::channel();
        for i in 0..200u32 {
            let tx = tx.clone();
            pool.submit(format!("k{i}"), Priority::Soon, move || Ok(i * 2), move |r| {
                tx.send(*r.unwrap()).unwrap();
            });
        }
        let mut got: Vec<u32> = (0..200).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        got.sort();
        assert_eq!(got, (0..200).map(|i| i * 2).collect::<Vec<_>>());
    }

    #[test]
    fn the_same_preview_asked_for_twice_is_made_once() {
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);

        let made = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel();
        for _ in 0..5 {
            let made = made.clone();
            let tx = tx.clone();
            pool.submit(
                "same".into(),
                Priority::Soon,
                move || {
                    made.fetch_add(1, Ordering::SeqCst);
                    Ok(7)
                },
                move |r| tx.send(*r.unwrap()).unwrap(),
            );
        }
        assert_eq!(pool.queued(), 1);
        open.send(()).unwrap();
        open_reserved.send(()).unwrap();

        let answers: Vec<u32> = (0..5).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        assert_eq!(answers, vec![7; 5], "all five are told");
        assert_eq!(made.load(Ordering::SeqCst), 1, "but it was made once");
    }

    #[test]
    fn the_queue_drains_by_priority_then_newest_first() {
        // One general worker (plus the reserved one), so the order is exact.
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);

        let (tx, rx) = mpsc::channel();
        let ask = |name: &'static str, priority| {
            let tx = tx.clone();
            pool.submit(name.into(), priority, move || Ok(0), move |_| tx.send(name).unwrap());
        };
        ask("later-old", Priority::Later);
        ask("soon-old", Priority::Soon);
        ask("soon-new", Priority::Soon);
        ask("later-new", Priority::Later);

        // Release only the general worker: the reserved one stays held, so
        // nothing here is made out of turn by it.
        open.send(()).unwrap();
        let order: Vec<&str> = (0..4).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        assert_eq!(order, vec!["soon-new", "soon-old", "later-new", "later-old"]);
        open_reserved.send(()).unwrap();
    }

    #[test]
    fn asking_again_at_a_higher_priority_moves_the_job_up() {
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);

        let (tx, rx) = mpsc::channel();
        let ask = |name: &'static str, priority| {
            let tx = tx.clone();
            pool.submit(name.into(), priority, move || Ok(0), move |_| tx.send(name).unwrap());
        };
        ask("ahead", Priority::Later);
        ask("visible", Priority::Soon);
        // The photo that was only being looked ahead to is now on screen.
        ask("ahead", Priority::Now);

        // The general worker alone, so the order is the queue's and not a
        // race between two threads.
        open.send(()).unwrap();
        let order: Vec<&str> = (0..3).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        // Made first, and both of its askers told, before the other job.
        assert_eq!(order, vec!["ahead", "ahead", "visible"]);
        open_reserved.send(()).unwrap();
    }

    #[test]
    fn a_failure_or_a_panic_is_reported_and_the_pool_keeps_working() {
        let pool = Pool::<u32>::new(2);
        let (tx, rx) = mpsc::channel();
        let t = tx.clone();
        pool.submit("fails".into(), Priority::Soon, || Err("bad file".into()), move |r| t.send(r.map(|v| *v)).unwrap());
        let t = tx.clone();
        pool.submit("panics".into(), Priority::Soon, || panic!("corrupt"), move |r| t.send(r.map(|v| *v)).unwrap());

        let mut errors: Vec<String> =
            (0..2).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err()).collect();
        errors.sort();
        assert_eq!(errors, vec!["bad file", "decoder panicked"]);

        // Still alive.
        pool.submit("after".into(), Priority::Soon, || Ok(9), move |r| tx.send(r.map(|v| *v)).unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Ok(9));
    }

    #[test]
    fn idle_work_waits_out_the_quiet_after_a_request_then_runs() {
        let pool = Pool::<u32>::new(4);
        let (tx, rx) = mpsc::channel();

        // A request, which starts the quiet period…
        let t = tx.clone();
        pool.submit("asked".into(), Priority::Soon, || Ok(1), move |_| t.send(("asked", Instant::now())).unwrap());
        // …and idle work submitted right behind it.
        let submitted = Instant::now();
        let t = tx.clone();
        pool.submit("ahead".into(), Priority::Idle, || Ok(2), move |_| t.send(("ahead", Instant::now())).unwrap());

        let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let second = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((first.0, second.0), ("asked", "ahead"));
        // Idle work held off for the whole quiet period, with workers free.
        assert!(second.1.duration_since(submitted) >= QUIET, "ran after only {:?}", second.1.duration_since(submitted));
        assert_eq!(pool.idle_remaining(), 0);
    }

    #[test]
    fn idle_work_never_makes_the_pool_busy_and_any_other_request_does() {
        // Work that stands aside while the pool is busy (the face scan) may
        // ask for idle work without standing aside for its own request.
        let pool = Pool::<u32>::new(4);
        let (tx, rx) = mpsc::channel();
        let (release, gate) = mpsc::channel::<()>();
        pool.submit("idle".into(), Priority::Idle, move || { gate.recv().ok(); Ok(1) }, move |_| tx.send(()).unwrap());
        assert!(!pool.busy(), "queued or under way, idle work leaves the pool free");
        release.send(()).unwrap();
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!pool.busy());
        pool.submit("later".into(), Priority::Later, || Ok(2), |_| {});
        assert!(pool.busy(), "a look-ahead starts the quiet period");
    }

    #[test]
    fn idle_work_is_held_to_its_share_of_the_workers() {
        // Five workers: one reserved, four general, so two may be idle-working.
        let pool = Pool::<u32>::new(5);
        let running = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel();
        for i in 0..12 {
            let (running, most, tx) = (running.clone(), most.clone(), tx.clone());
            pool.submit(
                format!("idle-{i}"),
                Priority::Idle,
                move || {
                    let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                    most.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(15));
                    running.fetch_sub(1, Ordering::SeqCst);
                    Ok(0)
                },
                move |_| tx.send(()).unwrap(),
            );
        }
        for _ in 0..12 {
            rx.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        assert_eq!(most.load(Ordering::SeqCst), 2, "never more than half the general workers");
    }

    #[test]
    fn idle_work_resumes_after_a_request_interrupts_it() {
        let pool = Pool::<u32>::with_idle_limit(5, 2);
        let (tx, rx) = mpsc::channel();
        for i in 0..20 {
            let tx = tx.clone();
            pool.submit(
                format!("idle-{i}"),
                Priority::Idle,
                || {
                    std::thread::sleep(Duration::from_millis(20));
                    Ok(0)
                },
                move |_| tx.send(()).unwrap(),
            );
        }
        // Let it get going, then ask for something — as a photographer
        // jumping to a photo partway through the folder being prepared does.
        std::thread::sleep(Duration::from_millis(60));
        assert!(pool.idle_remaining() > 0, "the interruption has to land mid-run to mean anything");
        let t = tx.clone();
        pool.submit("wanted".into(), Priority::Now, || Ok(1), move |_| t.send(()).unwrap());

        // The request, and then every one of the idle jobs, with nothing
        // further arriving to wake the workers up.
        for done in 0..21 {
            rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| panic!("stalled after {done} of 21"));
        }
        assert_eq!(pool.idle_remaining(), 0);
    }

    #[test]
    fn clearing_idle_work_leaves_everything_else_queued() {
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);
        let (tx, rx) = mpsc::channel();
        for (name, priority) in [("ahead-1", Priority::Idle), ("wanted", Priority::Soon), ("ahead-2", Priority::Idle)] {
            let tx = tx.clone();
            pool.submit(name.into(), priority, || Ok(1), move |r| tx.send((name, r.is_ok())).unwrap());
        }
        assert_eq!(pool.idle_remaining(), 2);
        pool.clear_idle();
        assert_eq!((pool.idle_remaining(), pool.queued()), (0, 1));

        // Both idle askers hear it was abandoned; the real request is made.
        let mut heard: Vec<(&str, bool)> = (0..2).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        heard.sort();
        assert_eq!(heard, vec![("ahead-1", false), ("ahead-2", false)]);
        open.send(()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), ("wanted", true));
        open_reserved.send(()).unwrap();
    }

    #[test]
    fn idle_work_someone_is_waiting_on_survives_clearing_and_goes_before_newer_idle_work() {
        // A judge's brightness read, blocked on, must not be dropped when
        // the folder's preparation is started again, nor end up behind it.
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);
        let (tx, rx) = mpsc::channel();
        let t = tx.clone();
        pool.submit_waited("judge".into(), Priority::Idle, || Ok(1), move |r| t.send(("judge", r.is_ok())).unwrap());
        pool.clear_idle();
        assert_eq!(pool.idle_remaining(), 1, "the waited-on job is kept");
        for name in ["warm-1", "warm-2"] {
            let tx = tx.clone();
            pool.submit(name.into(), Priority::Idle, || Ok(1), move |r| tx.send((name, r.is_ok())).unwrap());
        }

        // The general worker alone, so the order is exact.
        open.send(()).unwrap();
        let order: Vec<(&str, bool)> = (0..3).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        assert_eq!(order, vec![("judge", true), ("warm-2", true), ("warm-1", true)]);
        open_reserved.send(()).unwrap();
    }

    #[test]
    fn idle_work_nobody_is_waiting_on_is_still_dropped_by_clearing() {
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);
        let (tx, rx) = mpsc::channel();
        let t = tx.clone();
        pool.submit("ahead".into(), Priority::Idle, || Ok(1), move |r| t.send(("ahead", r.err())).unwrap());
        let t = tx.clone();
        pool.submit_waited("judge".into(), Priority::Idle, || Ok(1), move |r| t.send(("judge", r.err())).unwrap());
        pool.clear_idle();

        // Only the job nobody waits on is gone, and its asker is told.
        assert_eq!(pool.queued(), 1);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), ("ahead", Some(ABANDONED.to_string())));
        open.send(()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), ("judge", None));
        open_reserved.send(()).unwrap();
    }

    #[test]
    fn asking_again_as_idle_work_leaves_a_higher_priority_job_where_it_was() {
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);
        let (tx, rx) = mpsc::channel();
        let ask = |name: &'static str, priority| {
            let tx = tx.clone();
            pool.submit(name.into(), priority, move || Ok(0), move |_| tx.send(name).unwrap());
        };
        ask("older", Priority::Soon);
        ask("newer", Priority::Soon);
        let before = {
            let state = pool.shared.0.lock().unwrap();
            let job = state.queue.iter().find(|j| j.key == "older").unwrap();
            (job.priority, job.seq)
        };
        // A judge's spare-time read of a thumbnail browsing already wants.
        ask("older", Priority::Idle);
        let after = {
            let state = pool.shared.0.lock().unwrap();
            let job = state.queue.iter().find(|j| j.key == "older").unwrap();
            (job.priority, job.seq)
        };
        assert_eq!(after, before, "priority and place unchanged");

        open.send(()).unwrap();
        let order: Vec<&str> = (0..3).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        assert_eq!(order, vec!["newer", "older", "older"]);
        open_reserved.send(()).unwrap();
    }

    #[test]
    fn asking_again_at_look_ahead_still_moves_a_queued_on_screen_job_to_the_front() {
        // Holding an arrow key: looking ahead re-asks for frames already
        // queued for the screen, and that puts the next frame first.
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);
        let (tx, rx) = mpsc::channel();
        let ask = |name: &'static str, priority| {
            let tx = tx.clone();
            pool.submit(name.into(), priority, move || Ok(0), move |_| tx.send(name).unwrap());
        };
        ask("next", Priority::Now);
        ask("current", Priority::Now);
        let before = {
            let state = pool.shared.0.lock().unwrap();
            state.queue.iter().find(|j| j.key == "next").unwrap().seq
        };
        ask("next", Priority::Later);
        let (priority, after) = {
            let state = pool.shared.0.lock().unwrap();
            let job = state.queue.iter().find(|j| j.key == "next").unwrap();
            (job.priority, job.seq)
        };
        assert_eq!(priority, Priority::Now, "it keeps its higher priority");
        assert!(after > before, "and moves to the front");

        open.send(()).unwrap();
        let order: Vec<&str> = (0..3).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        assert_eq!(order, vec!["next", "next", "current"]);
        open_reserved.send(()).unwrap();
    }

    #[test]
    fn clearing_tells_whoever_was_waiting() {
        let pool = Pool::<u32>::new(2);
        let (open, open_reserved) = hold(&pool, 2);
        let (tx, rx) = mpsc::channel();
        for i in 0..3 {
            let tx = tx.clone();
            pool.submit(format!("k{i}"), Priority::Soon, || Ok(1), move |r| tx.send(r.is_err()).unwrap());
        }
        pool.clear_queued();
        assert_eq!(pool.queued(), 0);
        for _ in 0..3 {
            assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "each waiter hears it was abandoned");
        }
        open.send(()).unwrap();
        open_reserved.send(()).unwrap();
    }
}
