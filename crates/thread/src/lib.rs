use std::sync::mpsc::{Receiver, channel};
use std::thread::Result as ThreadResult;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ThreadError {
    #[error("failed to start a worker: {0}")]
    Spawn(String),
    #[error("the worker pool has shut down")]
    Closed,
    #[error("the task panicked")]
    Panicked,
}

impl ThreadError {
    pub fn code(&self) -> u16 {
        match self {
            ThreadError::Spawn(_) => 1,
            ThreadError::Closed => 2,
            ThreadError::Panicked => 3,
        }
    }
}

pub type Task = Box<dyn FnOnce() + Send + 'static>;

pub trait WorkerPool {
    fn workers(&self) -> usize;

    fn spawn(&self, task: Task) -> Result<(), ThreadError>;
}

/// How many workers can run at once.
pub fn available_parallelism() -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    return std::thread::available_parallelism().map_or(1, |n| n.get());
    #[cfg(target_arch = "wasm32")]
    rayon::current_num_threads().max(1)
}

/// The result of a task on a worker, waited for with [`JoinHandle::join`].
pub struct JoinHandle<T> {
    result: Receiver<ThreadResult<T>>,
}

impl<T> JoinHandle<T> {
    /// Blocks until the task is done. In the browser only a worker may block,
    /// not the page's main thread.
    pub fn join(self) -> Result<T, ThreadError> {
        match self.result.recv() {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err(ThreadError::Panicked),
            Err(_) => Err(ThreadError::Closed),
        }
    }

    /// The result if the task has finished.
    pub fn try_join(&self) -> Option<Result<T, ThreadError>> {
        match self.result.try_recv() {
            Ok(Ok(value)) => Some(Ok(value)),
            Ok(Err(_)) => Some(Err(ThreadError::Panicked)),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(ThreadError::Closed)),
        }
    }
}

/// Runs `task` on a worker; long-running loops are fine, each takes a worker
/// for as long as it runs.
pub fn spawn<T: Send + 'static>(
    name: impl Into<String>,
    task: impl FnOnce() -> T + Send + 'static,
) -> Result<JoinHandle<T>, ThreadError> {
    let (sender, result) = channel();
    let run = move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
        let _ = sender.send(outcome);
    };
    start(name.into(), Box::new(run))?;
    Ok(JoinHandle { result })
}

#[cfg(not(target_arch = "wasm32"))]
fn start(name: String, run: Task) -> Result<(), ThreadError> {
    std::thread::Builder::new()
        .name(name)
        .spawn(run)
        .map(|_| ())
        .map_err(|error| ThreadError::Spawn(error.to_string()))
}

#[cfg(target_arch = "wasm32")]
fn start(_name: String, run: Task) -> Result<(), ThreadError> {
    rayon::spawn(run);
    Ok(())
}

/// Starts the Web Workers of the pool, which tasks need before they run. Call
/// it once from JavaScript's `initThreadPool` and await it.
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_rayon::init_thread_pool;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Task, ThreadError, WorkerPool};
    use std::sync::mpsc::{Sender, channel};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    pub struct NativePool {
        sender: Option<Sender<Task>>,
        handles: Vec<JoinHandle<()>>,
    }

    impl NativePool {
        pub fn new(workers: usize) -> Result<Self, ThreadError> {
            let (sender, receiver) = channel::<Task>();
            let receiver = Arc::new(Mutex::new(receiver));
            let mut handles = Vec::with_capacity(workers);
            for index in 0..workers {
                let receiver = receiver.clone();
                let handle = std::thread::Builder::new()
                    .name(format!("worker-{index}"))
                    .spawn(move || {
                        loop {
                            let task = match receiver.lock() {
                                Ok(guard) => guard.recv(),
                                Err(_) => break,
                            };
                            match task {
                                Ok(task) => task(),
                                Err(_) => break,
                            }
                        }
                    })
                    .map_err(|error| ThreadError::Spawn(error.to_string()))?;
                handles.push(handle);
            }
            Ok(Self {
                sender: Some(sender),
                handles,
            })
        }
    }

    impl WorkerPool for NativePool {
        fn workers(&self) -> usize {
            self.handles.len()
        }

        fn spawn(&self, task: Task) -> Result<(), ThreadError> {
            self.sender
                .as_ref()
                .ok_or(ThreadError::Closed)?
                .send(task)
                .map_err(|_| ThreadError::Closed)
        }
    }

    impl Drop for NativePool {
        fn drop(&mut self) {
            self.sender.take();
            for handle in self.handles.drain(..) {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativePool;

/// The browser's pool as a [`WorkerPool`].
#[cfg(target_arch = "wasm32")]
pub struct RayonPool;

#[cfg(target_arch = "wasm32")]
impl WorkerPool for RayonPool {
    fn workers(&self) -> usize {
        rayon::current_num_threads()
    }

    fn spawn(&self, task: Task) -> Result<(), ThreadError> {
        rayon::spawn(task);
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn tasks_run_on_workers() {
        let pool = NativePool::new(2).unwrap();
        let (sender, receiver) = channel();
        pool.spawn(Box::new(move || sender.send(7).unwrap()))
            .unwrap();
        assert_eq!(receiver.recv().unwrap(), 7);
        assert_eq!(pool.workers(), 2);
    }

    #[test]
    fn a_spawned_task_joins_with_its_value() {
        let handle = spawn("test", || 40 + 2).unwrap();
        assert_eq!(handle.join(), Ok(42));
    }

    #[test]
    fn a_panicking_task_is_reported() {
        let handle = spawn("test", || panic!("boom")).unwrap();
        assert_eq!(handle.join(), Err(ThreadError::Panicked));
    }
}
