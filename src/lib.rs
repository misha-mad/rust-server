use std::{sync::mpsc, thread};
use std::sync::{Arc, Mutex};

/// A thread pool for executing tasks concurrently.
///
/// The `ThreadPool` struct manages a pool of worker threads. It allows you to submit
/// jobs (closures) that will be executed by these threads. This struct is useful for
/// managing a pool of threads that can handle multiple tasks concurrently, which is
/// ideal for server applications or any application that needs to perform many tasks
/// in parallel.
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

/// A type alias for a job. A job is a closure that can be executed by a worker thread.
type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    /// Creates a new `ThreadPool` with the specified number of worker threads.
    ///
    /// # Arguments
    ///
    /// * `size` - The number of threads to spawn in the pool.
    ///
    /// # Panics
    ///
    /// Panics if `size` is 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use rust_server::ThreadPool;
    /// let pool = ThreadPool::new(4);
    /// ```
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);
        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        ThreadPool { workers, sender: Some(sender) }
    }

    /// Executes a job on one of the worker threads in the pool.
    ///
    /// # Arguments
    ///
    /// * `f` - A closure to be executed by one of the worker threads.
    ///
    /// # Panics
    ///
    /// Panics if sending the job to the channel fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use rust_server::ThreadPool;
    /// let pool = ThreadPool::new(4);
    ///
    /// pool.execute(|| {
    ///     println!("Executing job in thread pool!");
    /// });
    /// ```
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        self.sender.as_ref().expect("Sender is missing").send(job).unwrap();
    }
}

impl Drop for ThreadPool {
    /// Cleans up the `ThreadPool`, joining all worker threads and closing the channel.
    ///
    /// This method is automatically called when the `ThreadPool` is dropped. It ensures
    /// that all worker threads are properly joined and the resources are cleaned up.
    fn drop(&mut self) {
        // Take the sender out of the `Option` to close the channel.
        drop(self.sender.take());

        // Wait for all worker threads to finish.
        for worker in &mut self.workers {
            println!("Shutting down worker {}", worker.id);

            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

/// A worker thread that executes jobs from a channel.
struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    /// Creates a new `Worker` with the specified ID and receiver.
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            // Wait for a job and execute it.
            let message = receiver.lock().unwrap().recv();

            match message {
                Ok(job) => {
                    println!("Worker {id} got a job; executing.");
                    job();
                }
                Err(_) => {
                    println!("Worker {id} disconnected; shutting down.");
                    break;
                }
            }
        });

        Worker { id, thread: Some(thread) }
    }
}
