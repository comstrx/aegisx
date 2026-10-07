use crate::core::env::Env;
use super::arch::{Resources, Sys};

impl Sys {

    pub fn resources () -> Resources {

        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let field = |key: &str| status.lines().find_map(|line| line.strip_prefix(key)?.split_whitespace().next()?.parse::<u64>().ok());

        let stat = std::fs::read_to_string("/proc/stat").unwrap_or_default();
        let machine_ticks = stat.lines().next().filter(|line| line.starts_with("cpu ")).map(|line| line.split_whitespace().skip(1).take(8).filter_map(|value| value.parse::<u64>().ok()).sum());

        let process = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
        let process_ticks = process.rsplit_once(')').map(|( _, rest )| rest.split_whitespace().skip(11).take(2).filter_map(|value| value.parse::<u64>().ok()).sum());

        Resources {
            rss_bytes     : field("VmRSS:").map(|kib| kib * 1024),
            threads       : field("Threads:"),
            open_fds      : std::fs::read_dir("/proc/self/fd").ok().map(|entries| entries.count() as u64),
            process_ticks,
            machine_ticks,
            cpus          : Env::cpus(),
        }

    }

    pub fn files ( wanted: u64 ) -> std::io::Result<u64> {

        rlimit::increase_nofile_limit(wanted)

    }

    pub fn cpu_percent ( previous: Resources, current: Resources ) -> Option<f64> {

        let ( before, after ) = ( previous.process_ticks?, current.process_ticks? );
        let ( total_before, total_after ) = ( previous.machine_ticks?, current.machine_ticks? );

        if total_after <= total_before { return None; }

        Some(after.saturating_sub(before) as f64 / (total_after - total_before) as f64 * current.cpus as f64 * 100.0)

    }

}
