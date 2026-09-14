use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError, mpsc};

use amiss_wire::model::Adapter;

use crate::scan::{PureFailure, Scanned, scan_pure};

/// One document parse for the pool: which staged document it belongs to,
/// the grammar, the bytes, and the embedded-code allowance the parse may use.
pub(crate) struct Task {
    pub(crate) position: usize,
    pub(crate) adapter: Adapter,
    pub(crate) body: Arc<[u8]>,
    pub(crate) allowance: u64,
}

pub(crate) struct Reply {
    pub(crate) position: usize,
    pub(crate) outcome: Result<Scanned, PureFailure>,
}

struct Queued {
    task: Task,
    reply: mpsc::Sender<Reply>,
}

struct Queue {
    tasks: Mutex<VecDeque<Queued>>,
    ready: Condvar,
}

struct Pool {
    queue: Arc<Queue>,
    threads: usize,
}

static POOL: OnceLock<Pool> = OnceLock::new();

/// Starts `threads` parser threads that live as long as the process. A
/// process that later refuses new threads, as the engine does when it
/// drops its process limit, keeps these. A second call changes nothing.
/// Returns how many threads run.
pub fn install(threads: NonZeroUsize) -> usize {
    POOL.get_or_init(|| {
        let queue = Arc::new(Queue {
            tasks: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
        });
        let mut started = 0_usize;
        for _thread in 0..threads.get() {
            let shared = Arc::clone(&queue);
            let spawned = std::thread::Builder::new()
                .name("amiss-parse".to_owned())
                .spawn(move || serve(&shared));
            if spawned.is_err() {
                break;
            }
            started = started.saturating_add(1);
        }
        Pool {
            queue,
            threads: started,
        }
    })
    .threads
}

/// How many pool threads run, zero when nothing was installed.
#[must_use]
pub fn installed() -> usize {
    POOL.get().map_or(0, |pool| pool.threads)
}

fn serve(queue: &Queue) {
    loop {
        let queued = {
            let mut tasks = queue.tasks.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if let Some(queued) = tasks.pop_front() {
                    break queued;
                }
                tasks = queue
                    .ready
                    .wait(tasks)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        let Queued { task, reply } = queued;
        let _gone = reply.send(execute(&task));
    }
}

fn execute(task: &Task) -> Reply {
    Reply {
        position: task.position,
        outcome: scan_pure(task.adapter, &task.body, task.allowance),
    }
}

/// Runs the tasks and returns every reply, in whatever order they finish.
/// The caller works the queue alongside the pool, so without a pool, or when
/// `parallel` is one, it does all of them itself.
pub(crate) fn run(tasks: Vec<Task>, parallel: NonZeroUsize) -> Vec<Reply> {
    let pool = POOL
        .get()
        .filter(|pool| pool.threads > 0 && parallel.get() > 1);
    let Some(pool) = pool else {
        return tasks.iter().map(execute).collect();
    };
    let expected = tasks.len();
    let (sender, receiver) = mpsc::channel();
    {
        let mut queue = pool
            .queue
            .tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        queue.extend(tasks.into_iter().map(|task| Queued {
            task,
            reply: sender.clone(),
        }));
    }
    pool.queue.ready.notify_all();
    drop(sender);
    let mut replies = Vec::with_capacity(expected);
    loop {
        let next = pool
            .queue
            .tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front();
        let Some(Queued { task, .. }) = next else {
            break;
        };
        replies.push(execute(&task));
    }
    while replies.len() < expected {
        let Ok(reply) = receiver.recv() else {
            break;
        };
        replies.push(reply);
    }
    replies
}
