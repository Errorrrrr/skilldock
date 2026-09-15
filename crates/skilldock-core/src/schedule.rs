use crate::model::Policy;
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Utc};

pub(crate) fn valid_daily_time(value: &str) -> bool {
    value.len() == 5
        && value.as_bytes()[2] == b':'
        && value
            .bytes()
            .enumerate()
            .all(|(i, c)| i == 2 || c.is_ascii_digit())
        && NaiveTime::parse_from_str(value, "%H:%M").is_ok()
}

pub(crate) fn next_check(policy: &Policy) -> String {
    next_check_at(policy, Local::now())
}

fn next_check_at<T: TimeZone>(policy: &Policy, now: DateTime<T>) -> String {
    if policy.mode == "off" {
        return String::new();
    }
    if let Some(time) = policy
        .daily_time
        .as_deref()
        .filter(|value| valid_daily_time(value))
    {
        let time = NaiveTime::parse_from_str(time, "%H:%M").unwrap();
        let zone = now.timezone();
        let mut date = now.date_naive();
        loop {
            let wall_time = date.and_time(time);
            // During a spring-forward gap use the first valid local minute.
            // During a repeated hour choose its first occurrence, once per day.
            for minute in 0..=180 {
                if let Some(candidate) = zone
                    .from_local_datetime(&(wall_time + Duration::minutes(minute)))
                    .earliest()
                {
                    if candidate > now {
                        return candidate.with_timezone(&Utc).to_rfc3339();
                    }
                    break;
                }
            }
            date = date.succ_opt().expect("next calendar day");
        }
    }
    (now.with_timezone(&Utc) + Duration::hours(policy.interval_hours as i64)).to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> Policy {
        Policy {
            mode: "notify".into(),
            interval_hours: 12,
            daily_time: Some("09:00".into()),
        }
    }
    #[test]
    fn daily_uses_local_calendar_and_never_repeats_today() {
        for (now, expected) in [
            ("2026-09-15T08:59:00+08:00", "2026-09-15T01:00:00+00:00"),
            ("2026-09-15T09:00:00+08:00", "2026-09-16T01:00:00+00:00"),
            ("2026-12-31T23:59:00+08:00", "2027-01-01T01:00:00+00:00"),
        ] {
            assert_eq!(
                next_check_at(&policy(), DateTime::parse_from_rfc3339(now).unwrap()),
                expected
            );
        }
    }
    #[test]
    fn old_policies_keep_interval_and_off_clears_schedule() {
        let mut policy: Policy =
            serde_json::from_str(r#"{"mode":"notify","intervalHours":12}"#).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-09-15T09:00:00+08:00").unwrap();
        assert_eq!(next_check_at(&policy, now), "2026-09-15T13:00:00+00:00");
        policy.mode = "off".into();
        assert_eq!(next_check_at(&policy, now), "");
    }
    #[test]
    fn validates_strict_hour_minute() {
        for value in ["00:00", "09:00", "23:59"] {
            assert!(valid_daily_time(value));
        }
        for value in ["", "9:00", "24:00", "12:60", "09:00:00", "ab:cd"] {
            assert!(!valid_daily_time(value));
        }
    }
}
