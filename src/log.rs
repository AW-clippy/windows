// Conditional logging macros - only log in debug builds
// In release builds, these compile to nothing (zero overhead)

#[cfg(debug_assertions)]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}

#[cfg(not(debug_assertions))]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if false {
            let _ = format_args!($($arg)*);
        }
    };
}

#[cfg(debug_assertions)]
macro_rules! debug_error {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[cfg(not(debug_assertions))]
macro_rules! debug_error {
    ($($arg:tt)*) => {
        if false {
            let _ = format_args!($($arg)*);
        }
    };
}

pub(crate) use debug_log;
pub(crate) use debug_error;
