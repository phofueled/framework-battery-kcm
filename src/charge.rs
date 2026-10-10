//! Keep charge-limit reads from cancelling the EC's volatile full-charge
//! override. Runtime state and flock coordinate D-Bus and schedule processes.

use crate::hardware::EcHardware;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions, Permissions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub struct BatteryState {
    pub present: bool,
    pub on_ac: bool,
    pub full: bool,
}

pub trait ChargeHardware {
    fn charge_limit(&self) -> Result<u8>;
    fn set_charge_limit(&self, limit: u8) -> Result<()>;
    fn charge_to_full_once(&self) -> Result<()>;
    fn finish_full_charge(&self) -> Result<()>;
    fn battery_state(&self) -> Result<BatteryState>;
}

impl ChargeHardware for EcHardware {
    fn charge_limit(&self) -> Result<u8> {
        self.charge_limit()
    }
    fn set_charge_limit(&self, limit: u8) -> Result<()> {
        self.set_charge_limit(limit)
    }
    fn charge_to_full_once(&self) -> Result<()> {
        self.charge_to_full_once()
    }
    fn finish_full_charge(&self) -> Result<()> {
        self.finish_full_charge()
    }
    fn battery_state(&self) -> Result<BatteryState> {
        self.battery_state()
    }
}

#[derive(Serialize, Deserialize)]
struct FullCharge {
    saved_limit: u8,
    ac_seen: bool,
    armed: bool,
}

pub struct ChargeControl {
    directory: PathBuf,
}

impl Default for ChargeControl {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("/run/framework-battery"),
        }
    }
}

impl ChargeControl {
    fn state_path(&self) -> PathBuf {
        self.directory.join("full-charge.json")
    }

    fn lock(&self) -> Result<File> {
        fs::create_dir_all(&self.directory)?;
        fs::set_permissions(&self.directory, Permissions::from_mode(0o700))?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(self.directory.join("control.lock"))?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error()).context("Cannot lock battery controls");
        }
        // Closing the file releases the process-shared lock on every exit path.
        Ok(file)
    }

    fn load(&self) -> Result<Option<FullCharge>> {
        match fs::read(self.state_path()) {
            Ok(bytes) => {
                let state: FullCharge = serde_json::from_slice(&bytes)
                    .context("Cannot read the one-time charge state")?;
                if state.saved_limit > 100 {
                    bail!("Invalid one-time charge limit");
                }
                Ok(Some(state))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("Cannot read the one-time charge state"),
        }
    }

    fn clear(&self) -> Result<()> {
        match fs::remove_file(self.state_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("Cannot clear the one-time charge state"),
        }
    }

    pub fn pending(&self) -> bool {
        self.state_path().is_file()
    }

    pub fn charge_limit(&self, hardware: &impl ChargeHardware) -> Result<u8> {
        let _lock = self.lock()?;
        match self.load()? {
            Some(state) if state.armed => Ok(state.saved_limit),
            Some(_) => {
                hardware.finish_full_charge()?;
                self.clear()?;
                hardware.charge_limit()
            }
            None => hardware.charge_limit(),
        }
    }

    pub fn set_charge_limit(&self, hardware: &impl ChargeHardware, limit: u8) -> Result<()> {
        if !(25..=100).contains(&limit) {
            bail!("Charge limit must be between 25% and 100%");
        }
        let _lock = self.lock()?;
        // A manual or scheduled change supersedes the one-time request.
        self.clear()?;
        hardware.set_charge_limit(limit)
    }

    pub fn charge_to_full_once(&self, hardware: &impl ChargeHardware) -> Result<()> {
        let _lock = self.lock()?;
        let saved_limit = self
            .load()?
            .map(|state| Ok(state.saved_limit))
            .unwrap_or_else(|| hardware.charge_limit())?;
        if saved_limit > 100 {
            bail!("The EC returned an invalid saved charge limit");
        }
        let battery = hardware.battery_state()?;
        if !battery.present {
            bail!("The EC reports no battery present");
        }
        let state = FullCharge {
            saved_limit,
            ac_seen: battery.on_ac,
            armed: false,
        };
        crate::storage::atomic_write(&self.state_path(), &serde_json::to_vec(&state)?, 0o600)?;
        if let Err(error) = hardware.charge_to_full_once() {
            self.clear()?;
            return Err(error);
        }
        let state = FullCharge {
            armed: true,
            ..state
        };
        if let Err(error) =
            crate::storage::atomic_write(&self.state_path(), &serde_json::to_vec(&state)?, 0o600)
        {
            hardware
                .finish_full_charge()
                .context("Cannot cancel the unsaved one-time request")?;
            self.clear()?;
            return Err(error);
        }
        Ok(())
    }

    /// Called only while a one-time request is pending. This uses battery
    /// memory-map reads, which do not touch the charge-limit override.
    pub fn poll_full_charge(&self, hardware: &impl ChargeHardware) -> Result<bool> {
        let _lock = self.lock()?;
        let Some(mut state) = self.load()? else {
            return Ok(false);
        };
        let battery = hardware.battery_state()?;
        if !state.armed || !battery.present || battery.full || (state.ac_seen && !battery.on_ac) {
            hardware.finish_full_charge()?;
            self.clear()?;
            return Ok(false);
        }
        if !state.ac_seen && battery.on_ac {
            state.ac_seen = true;
            crate::storage::atomic_write(&self.state_path(), &serde_json::to_vec(&state)?, 0o600)?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct FakeEc {
        saved: Cell<u8>,
        override_active: Cell<bool>,
        limit_reads: Cell<usize>,
        battery: Cell<BatteryState>,
        fail_override: Cell<bool>,
        fail_battery: Cell<bool>,
    }

    impl Default for FakeEc {
        fn default() -> Self {
            Self {
                saved: Cell::new(80),
                override_active: Cell::new(false),
                limit_reads: Cell::new(0),
                battery: Cell::new(BatteryState {
                    present: true,
                    on_ac: true,
                    full: false,
                }),
                fail_override: Cell::new(false),
                fail_battery: Cell::new(false),
            }
        }
    }

    impl ChargeHardware for FakeEc {
        fn charge_limit(&self) -> Result<u8> {
            // Reproduce Lilac firmware: a GET cancels the volatile override.
            self.override_active.set(false);
            self.limit_reads.set(self.limit_reads.get() + 1);
            Ok(self.saved.get())
        }
        fn set_charge_limit(&self, limit: u8) -> Result<()> {
            self.override_active.set(false);
            self.saved.set(limit);
            Ok(())
        }
        fn charge_to_full_once(&self) -> Result<()> {
            if self.fail_override.get() {
                bail!("EC rejected override");
            }
            self.override_active.set(true);
            Ok(())
        }
        fn finish_full_charge(&self) -> Result<()> {
            self.charge_limit().map(|_| ())
        }
        fn battery_state(&self) -> Result<BatteryState> {
            if self.fail_battery.get() {
                bail!("Battery status unavailable");
            }
            Ok(self.battery.get())
        }
    }

    fn control(directory: &tempfile::TempDir) -> ChargeControl {
        ChargeControl {
            directory: directory.path().to_owned(),
        }
    }

    #[test]
    fn refreshes_and_service_restarts_do_not_cancel_full_charge() {
        let directory = tempfile::tempdir().unwrap();
        let ec = FakeEc::default();
        control(&directory).charge_to_full_once(&ec).unwrap();
        let reads = ec.limit_reads.get();
        for _ in 0..20 {
            // Reconstruct the controller to simulate a restarted helper.
            let restarted = control(&directory);
            assert_eq!(restarted.charge_limit(&ec).unwrap(), 80);
            assert!(restarted.poll_full_charge(&ec).unwrap());
        }
        assert!(ec.override_active.get());
        assert_eq!(ec.limit_reads.get(), reads);
        assert_eq!(ec.saved.get(), 80);
        assert_eq!(
            fs::metadata(control(&directory).state_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn completion_and_unplug_restore_the_saved_limit() {
        for battery in [
            BatteryState {
                present: true,
                on_ac: true,
                full: true,
            },
            BatteryState {
                present: true,
                on_ac: false,
                full: false,
            },
        ] {
            let directory = tempfile::tempdir().unwrap();
            let c = control(&directory);
            let ec = FakeEc::default();
            c.charge_to_full_once(&ec).unwrap();
            ec.battery.set(battery);
            assert!(!c.poll_full_charge(&ec).unwrap());
            assert!(!c.pending());
            assert!(!ec.override_active.get());
            assert_eq!(ec.saved.get(), 80);
        }
    }

    #[test]
    fn request_on_battery_waits_for_ac_before_unplug_can_end_it() {
        let directory = tempfile::tempdir().unwrap();
        let c = control(&directory);
        let ec = FakeEc::default();
        ec.battery.set(BatteryState {
            present: true,
            on_ac: false,
            full: false,
        });
        c.charge_to_full_once(&ec).unwrap();
        assert!(c.poll_full_charge(&ec).unwrap());
        ec.battery.set(BatteryState {
            present: true,
            on_ac: true,
            full: false,
        });
        assert!(c.poll_full_charge(&ec).unwrap());
        ec.battery.set(BatteryState {
            present: true,
            on_ac: false,
            full: false,
        });
        assert!(!c.poll_full_charge(&ec).unwrap());
    }

    #[test]
    fn manual_or_scheduled_limit_supersedes_full_charge() {
        let directory = tempfile::tempdir().unwrap();
        let c = control(&directory);
        let ec = FakeEc::default();
        for next_limit in [90, 80] {
            c.charge_to_full_once(&ec).unwrap();
            assert!(c.set_charge_limit(&ec, 24).is_err());
            assert!(c.pending());
            c.set_charge_limit(&ec, next_limit).unwrap();
            assert!(!c.pending());
            assert!(!ec.override_active.get());
            assert_eq!(c.charge_limit(&ec).unwrap(), next_limit);
        }
    }

    #[test]
    fn failed_override_and_interrupted_request_do_not_leave_a_false_active_state() {
        let directory = tempfile::tempdir().unwrap();
        let c = control(&directory);
        let ec = FakeEc::default();
        ec.fail_override.set(true);
        assert!(c.charge_to_full_once(&ec).is_err());
        assert!(!c.pending());
        ec.fail_override.set(false);
        let interrupted = FullCharge {
            saved_limit: 80,
            ac_seen: true,
            armed: false,
        };
        crate::storage::atomic_write(
            &c.state_path(),
            &serde_json::to_vec(&interrupted).unwrap(),
            0o600,
        )
        .unwrap();
        assert!(!c.poll_full_charge(&ec).unwrap());
        assert!(!c.pending());
    }

    #[test]
    fn transient_battery_read_failure_retains_request_for_retry() {
        let directory = tempfile::tempdir().unwrap();
        let c = control(&directory);
        let ec = FakeEc::default();
        c.charge_to_full_once(&ec).unwrap();
        ec.fail_battery.set(true);
        assert!(c.poll_full_charge(&ec).is_err());
        assert!(c.pending());
        assert!(ec.override_active.get());
        ec.fail_battery.set(false);
        assert!(c.poll_full_charge(&ec).unwrap());
    }

    #[test]
    fn completion_preserves_a_limit_changed_by_another_tool() {
        let directory = tempfile::tempdir().unwrap();
        let c = control(&directory);
        let ec = FakeEc::default();
        c.charge_to_full_once(&ec).unwrap();
        ec.saved.set(90);
        ec.battery.set(BatteryState {
            present: true,
            on_ac: true,
            full: true,
        });
        c.poll_full_charge(&ec).unwrap();
        assert_eq!(ec.saved.get(), 90);
    }
}
