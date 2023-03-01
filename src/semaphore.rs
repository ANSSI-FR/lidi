use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
pub struct Semaphore(Arc<(Mutex<usize>, Condvar)>);

impl Semaphore {
    pub fn new(count: usize) -> Self {
        Self(Arc::new((Mutex::new(count), Condvar::new())))
    }

    pub(crate) fn acquire(&self) {
        let (lock, cv) = &*self.0;
        let mut counter = lock.lock().expect("acquire lock");
        while *counter == 0 {
            counter = cv
                .wait_while(counter, |counter| *counter == 0)
                .expect("condvar wait");
        }
        *counter = counter.checked_sub(1).expect("semaphore counter decrement");
    }

    pub(crate) fn release(&self) {
        let (lock, cv) = &*self.0;
        let mut counter = lock.lock().expect("acquire lock");
        *counter = counter.checked_add(1).expect("semaphore counter increment");
        cv.notify_one();
    }
}
