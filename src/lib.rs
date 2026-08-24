pub mod api;
pub mod app;
pub mod cli;
pub mod config;
pub mod os;
pub mod subscription;
pub mod ui;

#[cfg(test)]
pub mod testutil {
    use std::sync::{Mutex, OnceLock};

    pub fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
