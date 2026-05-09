#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(all(target_arch = "wasm32", not(test)))]
use core::panic::PanicInfo;
#[cfg(target_arch = "wasm32")]
use core::sync::atomic::{AtomicI32, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicI32, Ordering};

#[cfg(all(target_arch = "wasm32", not(test)))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

static COUNTER: AtomicI32 = AtomicI32::new(0);

#[no_mangle]
pub extern "C" fn increment() -> i32 {
    COUNTER.fetch_add(1, Ordering::SeqCst) + 1
}

#[no_mangle]
pub extern "C" fn decrement() -> i32 {
    COUNTER.fetch_sub(1, Ordering::SeqCst) - 1
}

#[no_mangle]
pub extern "C" fn get_value() -> i32 {
    COUNTER.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn reset() {
    COUNTER.store(0, Ordering::SeqCst);
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Use a mutex to serialize tests that share the global COUNTER
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_increment() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert_eq!(increment(), 1);
        assert_eq!(increment(), 2);
        assert_eq!(get_value(), 2);
    }

    #[test]
    fn test_decrement() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert_eq!(decrement(), -1);
        assert_eq!(decrement(), -2);
        assert_eq!(get_value(), -2);
    }

    #[test]
    fn test_reset() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        increment();
        increment();
        reset();
        assert_eq!(get_value(), 0);
    }
}
