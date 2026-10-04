use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::mem::MaybeUninit;

const WEEK_MINUTES: u32 = 7 * 24 * 60;
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn default_outside_limit() -> u8 {
    100
}
fn default_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub enabled: bool,
    #[serde(default = "default_outside_limit")]
    pub outside_limit: u8,
    pub entries: Vec<Entry>,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            outside_limit: 100,
            entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Bit 0 is Monday; bit 6 is Sunday. Selected days are start days.
    pub days: u8,
    /// Local minutes after midnight. The alias reads version 1 schedules.
    #[serde(alias = "minute")]
    pub start_minute: u16,
    /// None keeps the old event behavior until a new time window is saved.
    #[serde(default)]
    pub end_minute: Option<u16>,
    pub limit: u8,
}

impl Entry {
    fn duration(&self) -> Option<u32> {
        self.end_minute.map(|end| {
            let start = u32::from(self.start_minute);
            let end = u32::from(end);
            if end > start {
                end - start
            } else {
                1440 - start + end
            }
        })
    }
}

impl Schedule {
    pub fn has_legacy_entries(&self) -> bool {
        self.entries.iter().any(|entry| entry.end_minute.is_none())
    }

    pub fn validate(&self) -> Result<()> {
        if !(25..=100).contains(&self.outside_limit) {
            bail!("Outside-hours limit must be from 25% to 100%");
        }
        if self.entries.len() > 32 {
            bail!("A schedule can contain at most 32 profiles");
        }
        if self.enabled && !self.entries.iter().any(|entry| entry.enabled) {
            bail!("Enable at least one profile before enabling scheduling");
        }
        let legacy = self.has_legacy_entries();
        if legacy && self.entries.iter().any(|entry| entry.end_minute.is_some()) {
            bail!("Older event entries cannot be mixed with time-window profiles");
        }
        let mut legacy_events = HashSet::new();
        let mut occupied = vec![false; WEEK_MINUTES as usize];
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.name.chars().count() > 48 || entry.name.chars().any(char::is_control) {
                bail!("Profile {} has an invalid name", index + 1);
            }
            if entry.days == 0 || entry.days & !0x7f != 0 {
                bail!("Profile {} needs at least one valid weekday", index + 1);
            }
            if entry.start_minute >= 1440 || entry.end_minute.is_some_and(|end| end >= 1440) {
                bail!("Profile {} has an invalid time", index + 1);
            }
            if entry.end_minute == Some(entry.start_minute) {
                bail!("Profile {} needs different start and end times", index + 1);
            }
            if !(25..=100).contains(&entry.limit) {
                bail!("Profile {} must have a limit from 25% to 100%", index + 1);
            }
            if !entry.enabled {
                continue;
            }
            for day in 0..7 {
                if entry.days & (1 << day) == 0 {
                    continue;
                }
                let start = day * 1440 + u32::from(entry.start_minute);
                if let Some(duration) = entry.duration() {
                    for offset in 0..duration {
                        let minute = ((start + offset) % WEEK_MINUTES) as usize;
                        if occupied[minute] {
                            bail!("Enabled profiles have overlapping time windows");
                        }
                        occupied[minute] = true;
                    }
                } else if !legacy_events.insert(start) {
                    bail!("Two enabled entries use the same weekday and time");
                }
            }
        }
        Ok(())
    }

    pub fn effective_limit_at_week_minute(&self, now: u32) -> Option<u8> {
        if !self.enabled || now >= WEEK_MINUTES {
            return None;
        }
        if self.has_legacy_entries() {
            return self
                .entries
                .iter()
                .filter(|entry| entry.enabled)
                .flat_map(|entry| {
                    (0..7).filter_map(move |day| {
                        (entry.days & (1 << day) != 0)
                            .then_some((day * 1440 + u32::from(entry.start_minute), entry.limit))
                    })
                })
                .min_by_key(|(event, _)| (now + WEEK_MINUTES - event) % WEEK_MINUTES)
                .map(|(_, limit)| limit);
        }
        for entry in self.entries.iter().filter(|entry| entry.enabled) {
            let duration = entry.duration()?;
            for day in 0..7 {
                if entry.days & (1 << day) != 0 {
                    let start = day * 1440 + u32::from(entry.start_minute);
                    if (now + WEEK_MINUTES - start) % WEEK_MINUTES < duration {
                        return Some(entry.limit);
                    }
                }
            }
        }
        Some(self.outside_limit)
    }

    pub fn effective_limit_now(&self) -> Result<Option<u8>> {
        if !self.enabled {
            return Ok(None);
        }
        let mut now = MaybeUninit::<libc::timespec>::uninit();
        if unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, now.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error()).context("Cannot read the system clock");
        }
        let now = unsafe { now.assume_init() };
        Ok(self.effective_limit_at_week_minute(local_week_minute(now.tv_sec)?))
    }

    pub fn timer_unit(&self) -> Result<String> {
        self.validate()?;
        let mut unit = String::from("[Unit]\nDescription=Apply Framework battery charge schedule\n\n[Timer]\nUnit=framework-battery-schedule.service\nOnBootSec=30s\nPersistent=true\nAccuracySec=1s\n");
        for entry in self.entries.iter().filter(|entry| entry.enabled) {
            for (day, name) in DAYS.iter().enumerate() {
                if entry.days & (1 << day) == 0 {
                    continue;
                }
                unit.push_str(&format_calendar(name, entry.start_minute));
                if let Some(end) = entry.end_minute {
                    let end_day = (day + usize::from(end <= entry.start_minute)) % 7;
                    unit.push_str(&format_calendar(DAYS[end_day], end));
                }
            }
        }
        unit.push_str("\n[Install]\nWantedBy=timers.target\n");
        Ok(unit)
    }
}

extern "C" {
    fn tzset();
}

fn local_week_minute(timestamp: libc::time_t) -> Result<u32> {
    let mut local = MaybeUninit::<libc::tm>::uninit();
    // Refresh /etc/localtime after a timezone change. localtime_r alone may
    // reuse an older timezone; both libc calls serialize their internal state.
    unsafe { tzset() };
    if unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) }.is_null() {
        return Err(std::io::Error::last_os_error()).context("Cannot read the current local time");
    }
    let local = unsafe { local.assume_init() };
    if !(0..=6).contains(&local.tm_wday)
        || !(0..=23).contains(&local.tm_hour)
        || !(0..=59).contains(&local.tm_min)
    {
        bail!("The system returned an invalid local time");
    }
    Ok(((local.tm_wday + 6) % 7) as u32 * 1440 + local.tm_hour as u32 * 60 + local.tm_min as u32)
}

fn format_calendar(day: &str, minute: u16) -> String {
    format!(
        "OnCalendar={day} *-*-* {:02}:{:02}:00\n",
        minute / 60,
        minute % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(days: u8, start: u16, end: u16, limit: u8) -> Entry {
        Entry {
            name: String::new(),
            enabled: true,
            days,
            start_minute: start,
            end_minute: Some(end),
            limit,
        }
    }
    fn weekly() -> Schedule {
        Schedule {
            enabled: true,
            outside_limit: 100,
            entries: vec![
                profile(1, 8 * 60, 17 * 60, 80),
                profile(1 << 6, 20 * 60, 6 * 60, 60),
            ],
        }
    }

    #[test]
    fn local_time_handles_dst_week_boundaries_and_conversion_errors() {
        const CHILD_ZONE: &str = "FRAMEWORK_BATTERY_TEST_TIMEZONE";
        if let Ok(zone) = std::env::var(CHILD_ZONE) {
            let cases: &[(i64, u32)] = match zone.as_str() {
                "America/Toronto" => &[
                    (1_772_953_140, 8759), // Sunday 01:59 before spring DST.
                    (1_772_953_200, 8820), // Sunday 03:00 after the skipped hour.
                    (1_793_512_740, 8759), // Sunday 01:59 before autumn DST.
                    (1_793_512_800, 8700), // Sunday 01:00 after the repeated hour.
                    (1_773_633_540, 10079),
                    (1_773_633_600, 0),
                ],
                "Europe/Berlin" => &[
                    (1_774_745_940, 8759),
                    (1_774_746_000, 8820),
                    (1_792_889_940, 8819),
                    (1_792_890_000, 8760),
                ],
                "Asia/Kathmandu" => &[(1_773_598_440, 10079), (1_773_598_500, 0)],
                "UTC" => &[(1_773_619_140, 10079), (1_773_619_200, 0), (-1, 4319)],
                _ => panic!("Unexpected test timezone"),
            };
            for &(timestamp, minute) in cases {
                assert_eq!(
                    local_week_minute(timestamp).unwrap(),
                    minute,
                    "{zone}: {timestamp}"
                );
            }
            assert!(local_week_minute(i64::MAX).is_err());
            return;
        }
        // Each timezone gets its own process, avoiding global TZ mutations in
        // the parallel Rust test harness.
        for zone in ["America/Toronto", "Europe/Berlin", "Asia/Kathmandu", "UTC"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "schedule::tests::local_time_handles_dst_week_boundaries_and_conversion_errors",
                ])
                .env("TZ", zone)
                .env(CHILD_ZONE, zone)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{zone}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    #[test]
    fn windows_apply_across_midnight_and_week_boundary() {
        let mut schedule = weekly();
        schedule.outside_limit = 75;
        schedule.validate().unwrap();
        assert_eq!(schedule.effective_limit_at_week_minute(0), Some(60));
        assert_eq!(schedule.effective_limit_at_week_minute(6 * 60), Some(75));
        assert_eq!(schedule.effective_limit_at_week_minute(8 * 60), Some(80));
        assert_eq!(schedule.effective_limit_at_week_minute(17 * 60), Some(75));
        assert_eq!(
            schedule.effective_limit_at_week_minute(6 * 1440 + 20 * 60),
            Some(60)
        );
    }
    #[test]
    fn rejects_overlaps_equal_times_and_invalid_limits() {
        let mut schedule = weekly();
        schedule.entries.push(profile(1, 9 * 60, 10 * 60, 70));
        assert!(schedule.validate().is_err());
        schedule.entries.pop();
        schedule.entries[0].end_minute = Some(8 * 60);
        assert!(schedule.validate().is_err());
        schedule.entries[0].end_minute = Some(17 * 60);
        schedule.entries[0].limit = 24;
        assert!(schedule.validate().is_err());
        schedule.entries[0].limit = 80;
        schedule.outside_limit = 101;
        assert!(schedule.validate().is_err());
    }
    #[test]
    fn rejects_overnight_overlap_at_week_boundary_but_allows_adjacent_windows() {
        let mut schedule = weekly();
        schedule.entries.push(profile(1, 5 * 60, 7 * 60, 70));
        assert!(schedule.validate().is_err());
        schedule.entries[2].start_minute = 6 * 60;
        assert!(schedule.validate().is_ok());
    }
    #[test]
    fn disabled_profiles_do_not_apply_or_create_timers() {
        let mut schedule = weekly();
        schedule.entries[0].enabled = false;
        assert_eq!(schedule.effective_limit_at_week_minute(8 * 60), Some(100));
        assert!(!schedule
            .timer_unit()
            .unwrap()
            .contains("Mon *-*-* 08:00:00"));
        schedule.enabled = false;
        assert_eq!(schedule.effective_limit_at_week_minute(8 * 60), None);
    }
    #[test]
    fn timer_has_start_end_and_boot_triggers() {
        let unit = weekly().timer_unit().unwrap();
        assert!(unit.contains("OnBootSec=30s"));
        assert!(unit.contains("OnCalendar=Mon *-*-* 08:00:00"));
        assert!(unit.contains("OnCalendar=Mon *-*-* 17:00:00"));
        assert!(unit.contains("OnCalendar=Sun *-*-* 20:00:00"));
        assert!(unit.contains("OnCalendar=Mon *-*-* 06:00:00"));
    }
    #[test]
    fn version_one_events_keep_their_original_behavior() {
        let data = r#"{"enabled":true,"entries":[{"days":1,"minute":480,"limit":80},{"days":64,"minute":1200,"limit":60}]}"#;
        let schedule: Schedule = serde_json::from_str(data).unwrap();
        assert!(schedule.has_legacy_entries());
        schedule.validate().unwrap();
        assert_eq!(schedule.effective_limit_at_week_minute(0), Some(60));
        assert_eq!(schedule.effective_limit_at_week_minute(480), Some(80));
        assert!(!schedule.timer_unit().unwrap().contains("17:00:00"));
    }
}
