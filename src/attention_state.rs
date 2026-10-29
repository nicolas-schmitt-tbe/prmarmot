//! Attention state lives in `prmarmot-local` so the CLI can read the same
//! file; the app remains its only writer.

pub use prmarmot_local::attention_state::*;

pub fn until_tomorrow_local(now: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    until_tomorrow_in(now, &chrono::Local)
}

pub fn snooze_description_local(snooze: &Snooze) -> String {
    let offset = match snooze.condition {
        SnoozeCondition::Until { deadline } => deadline
            .with_timezone(&chrono::Local)
            .offset()
            .local_minus_utc(),
        _ => 0, // Conditional snoozes have no wall-clock timestamp.
    };
    snooze.description(offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone, Timelike, Utc};

    #[test]
    fn tomorrow_and_its_description_follow_the_local_calendar_across_dst() {
        // Exercise both European and US transitions; run this test in each
        // zone via TZ, without mutating the environment of parallel tests.
        for (month, day) in [(3, 7), (3, 28), (10, 24), (10, 31)] {
            let today = Local.with_ymd_and_hms(2026, month, day, 23, 30, 0).unwrap();
            let wake = until_tomorrow_local(today.with_timezone(&Utc));
            let local = wake.with_timezone(&Local);
            assert_eq!(local.date_naive(), today.date_naive().succ_opt().unwrap());
            assert_eq!((local.hour(), local.minute()), (9, 0));
            let snooze = AttentionState::snooze_for(
                &prmarmot_core::board::BoardRow::default(),
                SnoozeCondition::Until { deadline: wake },
            );
            assert_eq!(
                snooze_description_local(&snooze),
                format!("Snoozed until {} 09:00", local.date_naive())
            );
            assert!(!snooze.should_wake(None, wake - chrono::Duration::seconds(1)));
            assert!(snooze.should_wake(None, wake));
        }
    }
}
