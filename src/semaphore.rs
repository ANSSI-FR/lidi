use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
pub struct Semaphore(Arc<(Mutex<usize>, Condvar)>);

impl Semaphore {
    pub fn new(count: usize) -> Self {
        Self(Arc::new((Mutex::new(count), Condvar::new())))
    }

    pub(crate) fn acquire(&self) {
        let (lock, cv) = &*self.0;
        let mut counter = lock.lock().unwrap();
        while *counter == 0 {
            counter = cv.wait_while(counter, |counter| *counter == 0).unwrap();
        }
        *counter = counter
            .checked_sub(1)
            .expect(&format!("semaphore counter decrement failed: {}", counter));
    }

    pub(crate) fn release(&self) {
        let (lock, cv) = &*self.0;
        let mut counter = lock.lock().unwrap();
        *counter = counter
            .checked_add(1)
            .expect(&format!("semaphore counter increment failed: {}", counter));
        cv.notify_one();
    }
}
