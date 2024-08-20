use std::{sync::mpsc, thread};
use std::sync::{Arc, Mutex};
use std::thread::available_parallelism;

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
    /// If `size` is `None`, the number of worker threads will be equal to the number
    /// of logical cores on the system.
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
    /// let pool = ThreadPool::new(Some(4));
    /// ```
    pub fn new(size: Option<usize>) -> ThreadPool {
        let size = size.unwrap_or_else(|| available_parallelism().map(|n| n.get()).unwrap_or(1));
        println!("Creating a thread pool with {} worker threads.", size);
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
    /// let pool = ThreadPool::new(Some(4));
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
        if let Err(e) = self.sender.as_ref().expect("Sender is missing").send(job) {
            eprintln!("Failed to send job to thread pool: {}", e);
        }
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
                if let Err(e) = thread.join() {
                    eprintln!("Failed to join worker thread: {:?}", e);
                }
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
            let message = receiver.lock().expect("Failed to lock mutex").recv();

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn test_execute_jobs() {
        let pool = ThreadPool::new(Some(4));
        let (sender, receiver) = mpsc::channel();

        // Execute a job that sends a message to the channel
        pool.execute(move || {
            if let Err(e) = sender.send("Job 1 executed") {
                eprintln!("Failed to send message: {}", e);
            }
        });

        // Check that the job was executed
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(message) => assert_eq!(message, "Job 1 executed"),
            Err(e) => panic!("Failed to receive message: {}", e),
        }
    }

    #[test]
    fn test_pool_drops() {
        let (sender, receiver) = mpsc::channel();
        let pool = Arc::new(ThreadPool::new(Some(4)));
        let pool_clone = pool.clone();

        // Execute a job that sends a message to the channel
        pool_clone.execute(move || {
            if let Err(e) = sender.send("Job executed before drop") {
                eprintln!("Failed to send message: {}", e);
            }
        });

        // Drop the pool
        drop(pool);

        // Check that the job was executed before the pool was dropped
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(message) => assert_eq!(message, "Job executed before drop"),
            Err(e) => panic!("Failed to receive message: {}", e),
        }
    }

    #[test]
    fn test_delayed_job() {
        let pool = ThreadPool::new(Some(4));
        let (sender, receiver) = mpsc::channel();

        // Execute a job that has a delay
        pool.execute(move || {
            thread::sleep(Duration::from_secs(2));

            if let Err(e) = sender.send("Delayed job executed") {
                eprintln!("Failed to send message: {}", e);
            }
        });

        // Check that the job was executed after delay
        match receiver.recv_timeout(Duration::from_secs(3)) {
            Ok(message) => assert_eq!(message, "Delayed job executed"),
            Err(e) => panic!("Failed to receive message: {}", e),
        }
    }
}
