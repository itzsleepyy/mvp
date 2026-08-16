//! Minimal debug logging, enabled with `WAITSTATE_DEBUG=1`. Nothing is ever
//! logged by default, and lifecycle events are the only thing we ever log
//! about agents — never prompts, tool output or code.

/// Logs to stderr only when `WAITSTATE_DEBUG=1` is set.
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {{
        if std::env::var("WAITSTATE_DEBUG").is_ok_and(|v| v == "1") {
            eprintln!("[waitstate] {}", format_args!($($arg)*));
        }
    }};
}
