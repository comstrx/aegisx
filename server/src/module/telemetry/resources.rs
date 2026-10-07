use std::sync::Mutex;
use serde_json::{Value, json};
#[derive(Default)]
pub struct Resources { previous: Mutex<Option<(u64,u64)>> }
impl Resources {
    pub fn snapshot ( &self ) -> Value {
        let status=std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let number=|key:&str|status.lines().find_map(|line|line.strip_prefix(key)?.split_whitespace().next()?.parse::<u64>().ok());
        let stat=std::fs::read_to_string("/proc/stat").unwrap_or_default();
        let cores=stat.lines().filter(|line|line.starts_with("cpu") && line.as_bytes().get(3).is_some_and(u8::is_ascii_digit)).count();
        let total: u64=stat.lines().next().unwrap_or("").split_whitespace().skip(1).take(8).filter_map(|value|value.parse::<u64>().ok()).sum();
        let process=std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
        let ticks=process.rsplit_once(')').map(|(_,values)|values.split_whitespace().skip(11).take(2).filter_map(|value|value.parse::<u64>().ok()).sum::<u64>());
        let mut previous=self.previous.lock().unwrap_or_else(|error|error.into_inner());
        let cpu=ticks.and_then(|ticks|previous.filter(|(old_total,_)|total>*old_total).map(|(old_total,old_ticks)|
            ticks.saturating_sub(old_ticks) as f64 / (total-old_total) as f64 * cores as f64 * 100.0));
        if let Some(ticks)=ticks { *previous=Some((total,ticks)); }
        json!({"rss_bytes":number("VmRSS:").map(|value|value*1024),"threads":number("Threads:"),
            "process_cpu_percent":cpu,"logical_cpus":cores,
            "open_fds":std::fs::read_dir("/proc/self/fd").ok().map(|entries|entries.count()),
            "scope":"Linux process; CPU percent uses one logical CPU as 100%, not container quota"})
    }
}
