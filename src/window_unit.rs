use std::time::Duration;
use serde::Deserialize;

//TODO enum is serde library coupled -> decouple using DTO strategy
#[derive(Deserialize, PartialEq, Copy, Clone, Debug)]
pub enum WindowUnit {
    Days,
    Hours,
    Minutes,
    Seconds,
}

impl WindowUnit {
    pub fn in_seconds(self) -> u64 {
        match self {
            WindowUnit::Days => 86400,
            WindowUnit::Hours => 3600,
            WindowUnit::Minutes => 60,
            WindowUnit::Seconds => 1,
        }
    }

    pub fn elapsed_time_units(self, elapsed: Duration) -> u64 {
        elapsed.as_secs() / self.in_seconds()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod duration_of_one_unit {
        use super::*;

        #[test]
        fn should_measure_one_day_as_86400_seconds() {
            assert_eq!(WindowUnit::Days.in_seconds(), 86400);
        }

        #[test]
        fn should_measure_one_hour_as_3600_seconds() {
            assert_eq!(WindowUnit::Hours.in_seconds(), 3600);
        }

        #[test]
        fn should_measure_one_minute_as_60_seconds() {
            assert_eq!(WindowUnit::Minutes.in_seconds(), 60);
        }

        #[test]
        fn should_measure_one_second_as_1_second() {
            assert_eq!(WindowUnit::Seconds.in_seconds(), 1);
        }
    }

    mod counting_elapsed_units {
        use super::*;

        #[test]
        fn should_count_only_whole_units() {
            assert_eq!(WindowUnit::Minutes.elapsed_time_units(Duration::from_secs(59)), 0);
            assert_eq!(WindowUnit::Minutes.elapsed_time_units(Duration::from_secs(60)), 1);
            assert_eq!(WindowUnit::Minutes.elapsed_time_units(Duration::from_secs(119)), 1);
            assert_eq!(WindowUnit::Minutes.elapsed_time_units(Duration::from_secs(120)), 2);
        }

        #[test]
        fn should_discard_the_remainder_of_a_partial_unit() {
            assert_eq!(WindowUnit::Hours.elapsed_time_units(Duration::from_secs(3599)), 0);
            assert_eq!(WindowUnit::Days.elapsed_time_units(Duration::from_secs(86399)), 0);
        }

        #[test]
        fn should_discard_sub_second_precision() {
            assert_eq!(WindowUnit::Seconds.elapsed_time_units(Duration::from_millis(999)), 0);
            assert_eq!(WindowUnit::Seconds.elapsed_time_units(Duration::from_millis(1999)), 1);
        }

        #[test]
        fn should_count_every_second_when_unit_is_seconds() {
            assert_eq!(WindowUnit::Seconds.elapsed_time_units(Duration::from_secs(7)), 7);
        }
    }
}
