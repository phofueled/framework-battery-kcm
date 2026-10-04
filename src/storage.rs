use crate::hardware::EcHardware;
use crate::schedule::Schedule;
use anyhow::{bail, Context, Result};
use std::fs::{self, File, Permissions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;

const STATE_DIR: &str = "/var/lib/framework-battery";
const CONFIG: &str = "/var/lib/framework-battery/schedule.json";
const TIMER: &str = "/etc/systemd/system/framework-battery-schedule.timer";
const TIMER_NAME: &str = "framework-battery-schedule.timer";
const OVERRIDE_MARKER: &str = "/var/lib/framework-battery/override-confirmed";

pub fn override_confirmed() -> bool {
    Path::new(OVERRIDE_MARKER).is_file()
}

pub fn verify_override() -> Result<()> {
    EcHardware::new()?.charge_to_full_once()?;
    fs::create_dir_all(STATE_DIR)?;
    fs::set_permissions(STATE_DIR, Permissions::from_mode(0o700))?;
    atomic_write(Path::new(OVERRIDE_MARKER), b"confirmed\n", 0o600)?;
    Ok(())
}

pub fn hardware_self_test() -> Result<()> {
    let hardware = EcHardware::new()?;
    let original = hardware.charge_limit()?;
    let temporary = if original == 100 { 99 } else { original + 1 };
    let changed = hardware.set_charge_limit(temporary);
    let restored = hardware.set_charge_limit(original);
    restored.context("Could not restore the original charge limit")?;
    changed.context("Could not apply the temporary test limit")?;
    verify_override()?;
    println!("Charge limit changed to {temporary}% and restored to {original}%; one-time override confirmed");
    Ok(())
}

pub fn load() -> Result<Schedule> {
    match fs::read_to_string(CONFIG) {
        Ok(data) => {
            let schedule: Schedule =
                serde_json::from_str(&data).context("Invalid saved schedule")?;
            schedule.validate()?;
            Ok(schedule)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Schedule::default()),
        Err(error) => Err(error).context("Cannot read the saved schedule"),
    }
}

fn atomic_write(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    let parent = path.parent().context("Output path has no parent")?;
    let mut temporary = NamedTempFile::new_in(parent).context("Cannot create temporary file")?;
    temporary
        .as_file()
        .set_permissions(Permissions::from_mode(mode))?;
    temporary.write_all(data)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .context("Cannot replace saved file")?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn systemctl(args: &[&str]) -> Result<()> {
    let result = Command::new("/usr/bin/systemctl")
        .args(args)
        .output()
        .context("Cannot run systemctl")?;
    if !result.status.success() {
        bail!(
            "systemctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(())
}

pub fn save(schedule: &Schedule) -> Result<()> {
    schedule.validate()?;
    fs::create_dir_all(STATE_DIR).context("Cannot create schedule state directory")?;
    fs::set_permissions(STATE_DIR, Permissions::from_mode(0o700))?;

    if schedule.enabled {
        let timer = schedule.timer_unit()?;
        atomic_write(Path::new(TIMER), timer.as_bytes(), 0o644)?;
    }
    atomic_write(
        Path::new(CONFIG),
        serde_json::to_vec_pretty(schedule)?.as_slice(),
        0o600,
    )?;

    if schedule.enabled {
        // enable reloads the manager; restart also starts an inactive timer.
        systemctl(&["enable", TIMER_NAME])?;
        systemctl(&["restart", TIMER_NAME])?;
        apply_current(schedule)?;
    } else if Path::new(TIMER).exists() {
        systemctl(&["disable", "--now", TIMER_NAME])?;
    }
    Ok(())
}

pub fn apply_current(schedule: &Schedule) -> Result<()> {
    if let Some(limit) = schedule.effective_limit_now()? {
        EcHardware::new()?.set_charge_limit(limit)?;
    }
    Ok(())
}

pub fn apply_saved() -> Result<()> {
    apply_current(&load()?)
}
