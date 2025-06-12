use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};

pub struct Mutex<T> {
    value: UnsafeCell<T>,
    #[cfg(not(target_env = "polkavm"))]
    flag: core::sync::atomic::AtomicBool,
}

// SAFETY: It's always safe to send this mutex to another thread.
unsafe impl<T> Send for Mutex<T> where T: Send {}

// SAFETY: It's always safe to access this mutex from multiple threads.
unsafe impl<T> Sync for Mutex<T> where T: Send {}

pub struct MutexGuard<'a, T: 'a>(&'a Mutex<T>);

impl<T> Mutex<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
        Mutex {
            value: UnsafeCell::new(value),
            #[cfg(not(target_env = "polkavm"))]
            flag: core::sync::atomic::AtomicBool::new(false),
        }
    }

    #[inline]
    pub fn lock(&self) -> MutexGuard<T> {
        #[cfg(not(target_env = "polkavm"))]
        {
            use core::sync::atomic::Ordering;
            while self
                .flag
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {}
        }

        MutexGuard(self)
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(not(target_env = "polkavm"))]
        self.0.flag.store(false, core::sync::atomic::Ordering::Release);
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: We've locked the mutex, so we can access the data.
        unsafe { &*self.0.value.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: We've locked the mutex, so we can access the data.
        unsafe { &mut *self.0.value.get() }
    }
}
