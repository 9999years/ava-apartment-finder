use std::fmt::Display;

use chrono::Duration;

pub struct PrettyDuration(pub Duration);

impl Display for PrettyDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const MINS_PER_HOUR: i64 = 60;
        const MINS_PER_DAY: i64 = 24 * MINS_PER_HOUR;

        let minutes = self.0.num_minutes();
        let days = minutes / MINS_PER_DAY;
        let minutes = minutes - days * MINS_PER_DAY;

        let hours = minutes / MINS_PER_HOUR;
        let minutes = minutes - hours * MINS_PER_HOUR;

        let mut written = false;

        if days != 0 {
            write!(f, "{days} days")?;
            written = true;
        }

        if written {
            write!(f, " ")?;
        }

        if written || hours != 0 {
            write!(f, "{hours} hrs")?;
            written = true;
        }

        if written {
            write!(f, " ")?;
        }

        if written || minutes != 0 {
            write!(f, "{minutes} mins")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pretty_duration_mins() {
        assert_eq!(
            &PrettyDuration(Duration::minutes(15)).to_string(),
            "15 mins"
        );
    }

    #[test]
    fn test_pretty_duration_hours() {
        assert_eq!(
            &PrettyDuration(Duration::minutes(65)).to_string(),
            "1 hrs 5 mins"
        );
    }

    #[test]
    fn test_pretty_duration_0_hrs() {
        assert_eq!(
            &PrettyDuration(Duration::minutes(1 * (24 * 60) + 34)).to_string(),
            "1 days 0 hrs 34 mins"
        );
    }

    #[test]
    fn test_pretty_duration_days() {
        assert_eq!(
            &PrettyDuration(Duration::minutes(1 * (24 * 60) + 5 * 60 + 34)).to_string(),
            "1 days 5 hrs 34 mins"
        );
    }

    #[test]
    fn test_pretty_duration_0_mins() {
        assert_eq!(
            &PrettyDuration(Duration::minutes(1 * (24 * 60))).to_string(),
            "1 days 0 hrs 0 mins"
        );
    }
}
