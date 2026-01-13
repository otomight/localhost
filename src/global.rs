//! List of all constants or other elements in a global scope.

pub const CONFIG_PATH: &str = "config.conf";
pub const MAX_EVENTS: usize = 255;
pub const BUFFER_SIZE: usize = 8192;
pub const RATELIMITER_WINDOW: u64 = 1000;
pub const RATELIMITER_REQUEST_NUMBER: i32 = 255;




pub static ERROR_400_HTML: &[u8] = include_bytes!("../template/errors/400.html");
pub static ERROR_403_HTML: &[u8] = include_bytes!("../template/errors/403.html");
pub static ERROR_404_HTML: &[u8] = include_bytes!("../template/errors/404.html");
pub static ERROR_405_HTML: &[u8] = include_bytes!("../template/errors/405.html");
pub static ERROR_413_HTML: &[u8] = include_bytes!("../template/errors/413.html");
pub static ERROR_429_HTML: &[u8] = include_bytes!("../template/errors/429.html");
pub static ERROR_500_HTML: &[u8] = include_bytes!("../template/errors/500.html");

