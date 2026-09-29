use anyhow::{bail, Result};
use chrono::{DateTime, Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const WEEK_MINUTES: u32 = 7 * 24 * 60;
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub enabled: bool,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Bit 0 is Monday; bit 6 is Sunday.
    pub days: u8,
    /// Local minutes after midnight.
    pub minute: u16,
    pub limit: u8,
}

impl Schedule {
    pub fn validate(&self) -> Result<()> {
        if self.enabled && self.entries.is_empty() {
            bail!("Add at least one schedule entry before enabling scheduling");
        }
        if self.entries.len() > 32 {
            bail!("A schedule can contain at most 32 entries");
        }
        let mut occupied = HashSet::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.days == 0 || entry.days & !0x7f != 0 {
                bail!("Entry {} needs at least one valid weekday", index + 1);
            }
            if entry.minute >= 1440 {
                bail!("Entry {} has an invalid time", index + 1);
            }
            if !(25..=100).contains(&entry.limit) {
                bail!("Entry {} must have a limit from 25% to 100%", index + 1);
            }
            for day in 0..7 {
                if entry.days & (1 << day) != 0 && !occupied.insert((day, entry.minute)) {
                    bail!("Two entries use the same weekday and time");
                }
            }
        }
        Ok(())
    }

    pub fn effective_limit_at_week_minute(&self, now: u32) -> Option<u8> {
        if !self.enabled || now >= WEEK_MINUTES {
            return None;
        }
        self.entries
            .iter()
            .flat_map(|entry| {
                (0..7).filter_map(move |day| {
                    (entry.days & (1 << day) != 0)
                        .then_some((day * 1440 + u32::from(entry.minute), entry.limit))
                })
            })
            .min_by_key(|(event, _)| (now + WEEK_MINUTES - event) % WEEK_MINUTES)
            .map(|(_, limit)| limit)
    }

    pub fn effective_limit_now(&self) -> Option<u8> {
        let now: DateTime<Local> = Local::now();
        let week_minute =
            now.weekday().num_days_from_monday() * 1440 + now.hour() * 60 + now.minute();
        self.effective_limit_at_week_minute(week_minute)
    }

    pub fn timer_unit(&self) -> Result<String> {
        self.validate()?;
        let mut unit = String::from(
            "[Unit]\nDescription=Apply Framework battery charge schedule\n\n[Timer]\nUnit=framework-battery-schedule.service\nOnBootSec=30s\nPersistent=true\nAccuracySec=1s\n",
        );
        for entry in &self.entries {
            let hour = entry.minute / 60;
            let minute = entry.minute % 60;
            for (day, name) in DAYS.iter().enumerate() {
                if entry.days & (1 << day) != 0 {
                    unit.push_str(&format!(
                        "OnCalendar={name} *-*-* {hour:02}:{minute:02}:00\n"
                    ));
                }
            }
        }
        unit.push_str("\n[Install]\nWantedBy=timers.target\n");
        Ok(unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weekly() -> Schedule {
        Schedule {
            enabled: true,
            entries: vec![
                Entry {
                    days: 0b0000_0001,
                    minute: 8 * 60,
                    limit: 80,
                },
                Entry {
                    days: 0b0100_0000,
                    minute: 20 * 60,
                    limit: 60,
                },
            ],
        }
    }

    #[test]
    fn selects_most_recent_entry_across_week_boundary() {
        let schedule = weekly();
        assert_eq!(schedule.effective_limit_at_week_minute(0), Some(60));
        assert_eq!(schedule.effective_limit_at_week_minute(8 * 60), Some(80));
        assert_eq!(
            schedule.effective_limit_at_week_minute(6 * 1440 + 20 * 60),
            Some(60)
        );
    }

    #[test]
    fn rejects_duplicate_days_times_and_invalid_limits() {
        let mut schedule = weekly();
        schedule.entries.push(Entry {
            days: 1,
            minute: 480,
            limit: 70,
        });
        assert!(schedule.validate().is_err());
        schedule.entries.pop();
        schedule.entries[0].limit = 24;
        assert!(schedule.validate().is_err());
        schedule.entries[0].limit = 80;
        schedule.entries[0].minute = 1440;
        assert!(schedule.validate().is_err());
    }

    #[test]
    fn disabled_schedule_does_not_apply_a_limit() {
        let mut schedule = weekly();
        schedule.enabled = false;
        assert_eq!(schedule.effective_limit_at_week_minute(500), None);
    }

    #[test]
    fn timer_has_boot_and_calendar_triggers() {
        let unit = weekly().timer_unit().unwrap();
        assert!(unit.contains("OnBootSec=30s"));
        assert!(unit.contains("OnCalendar=Mon *-*-* 08:00:00"));
        assert!(unit.contains("OnCalendar=Sun *-*-* 20:00:00"));
    }
}
