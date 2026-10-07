use std::cell::Cell;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::arch::Clock;

static ANCHOR: OnceLock<( Instant, u64 )> = OnceLock::new();

thread_local! {
    static RECENT: Cell<Instant> = Cell::new(Instant::now());
}

impl Clock {

    pub fn recent () -> Instant {

        RECENT.with(Cell::get)

    }

    pub fn tick () -> Instant {

        let now = Instant::now();

        RECENT.with(|recent| recent.set(now));

        now

    }

    pub fn now_ms () -> u64 {

        SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis() as u64).unwrap_or(0)

    }

    pub fn instant () -> Instant {

        Instant::now()

    }

    pub fn wall_ms ( instant: Instant ) -> u64 {

        let ( base, wall ) = *ANCHOR.get_or_init(|| ( Instant::now(), Self::now_ms() ));

        match instant.checked_duration_since(base) {
            Some(ahead) => wall + ahead.as_millis() as u64,
            None => wall.saturating_sub(base.saturating_duration_since(instant).as_millis() as u64),
        }

    }

    pub fn stamp ( instant: Instant ) -> u64 {

        let ( base, _ ) = *ANCHOR.get_or_init(|| ( Instant::now(), Self::now_ms() ));

        instant.saturating_duration_since(base).as_millis() as u64 + 1

    }

    pub fn elapsed_ms ( since: Instant ) -> u64 {

        since.elapsed().as_millis() as u64

    }

    pub fn elapsed_us ( since: Instant ) -> u64 {

        since.elapsed().as_micros() as u64

    }

    pub fn millis ( value: u64 ) -> Duration {

        Duration::from_millis(value)

    }

    pub fn civil ( days: i64 ) -> ( i64, u32, u32 ) {

        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;

        ( if month <= 2 { year + 1 } else { year }, month, day )

    }

}
