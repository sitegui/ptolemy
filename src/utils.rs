use std::time::Instant;

pub struct DebugTime {
    start: Instant,
}

impl DebugTime {
    pub fn new() -> Self {
        DebugTime {
            start: Instant::now(),
        }
    }

    pub fn msg<T: std::fmt::Display>(&mut self, s: T) {
        let dt = Instant::now() - self.start;
        println!("[{:6.1}s] {}", dt.as_secs_f32(), s);
    }
}

pub fn format_bytes(n: u64) -> String {
    if n < 1000 {
        format!("{}B", n)
    } else if n < 1000 * 1024 {
        format!("{:.1}kiB", n as f32 / 1024.)
    } else if n < 1000 * 1024 * 1024 {
        format!("{:.1}MiB", n as f32 / 1024. / 1024.)
    } else {
        format!("{:.1}GiB", n as f32 / 1024. / 1024. / 1024.)
    }
}

pub fn format_num(n: usize) -> String {
    if n < 1000 {
        format!("{}", n)
    } else if n < 1000 * 1000 {
        format!("{:.1}k", n as f32 / 1000.)
    } else {
        format!("{:.1}M", n as f32 / 1000. / 1000.)
    }
}
