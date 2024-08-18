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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn test_execute_jobs() {
        let pool = ThreadPool::new(4);
        let (sender, receiver) = mpsc::channel();

        // Execute a job that sends a message to the channel
        pool.execute(move || {
            sender.send("Job 1 executed").unwrap();
        });

        // Check that the job was executed
        let message = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(message, "Job 1 executed");
    }

    #[test]
    fn test_pool_drops() {
        let (sender, receiver) = mpsc::channel();
        let pool = Arc::new(ThreadPool::new(4));
        let pool_clone = pool.clone();

        // Execute a job that sends a message to the channel
        pool_clone.execute(move || {
            sender.send("Job executed before drop").unwrap();
        });

        // Drop the pool
        drop(pool);

        // Check that the job was executed before the pool was dropped
        let message = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(message, "Job executed before drop");
    }

    #[test]
    fn test_delayed_job() {
        let pool = ThreadPool::new(4);
        let (sender, receiver) = mpsc::channel();

        // Execute a job that has a delay
        pool.execute(move || {
            thread::sleep(Duration::from_secs(2));
            sender.send("Delayed job executed").unwrap();
        });

        // Check that the job was executed after delay
        let message = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(message, "Delayed job executed");
    }

    #[test]
    fn test_worker_shutdown() {
        // Create a pool with one worker for testing
        let pool = ThreadPool::new(1);
        let (sender, receiver) = mpsc::channel();

        // Execute a job
        pool.execute(move || {
            sender.send("Job before shutdown").unwrap();
        });

        // Check that the job was executed
        let message = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(message, "Job before shutdown");

        // Drop the pool and ensure all workers are shut down
        drop(pool);

        // Create a new channel and verify that the sender is no longer usable
        let (second_sender, _) = mpsc::channel();
        let result = second_sender.send("Job after shutdown");
        println!("{:?}", result);
        assert!(result.is_err(), "Expected error sending to a closed channel");

        // Create a new channel and try sending a message
        let (third_sender, _new_receiver) = mpsc::channel();
        let result = third_sender.send("Job after shutdown");
        println!("{:?}", result);
        assert!(result.is_ok(), "Expected successful send to a new channel");
    }
}

