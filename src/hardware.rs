//! Minimal Linux `/dev/cros_ec` transport for Framework charge-limit commands.
//! Command IDs, modes, and the ioctl layout follow the BSD-3-Clause licensed
//! Framework System `framework_lib` 0.6.6 and Linux cros_ec_dev interface.

use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

const DEVICE: &str = "/dev/cros_ec";
const CHARGE_LIMIT_CONTROL: u32 = 0x3e03;
const SET: u8 = 0x02;
const GET: u8 = 0x08;
const OVERRIDE: u8 = 0x80;
const DISABLE: u8 = 0x01;
// _IOWR(0xEC, 0, struct cros_ec_command): the kernel header is five u32s.
const CROS_EC_DEV_IOCXCMD: libc::c_ulong = 0xc014_ec00;
// _IOWR(0xEC, 1, struct cros_ec_readmem).
const CROS_EC_DEV_IOCRDMEM: libc::c_ulong = 0xc108_ec01;
const BATTERY_FLAGS_OFFSET: u32 = 0x4c;
const BATTERY_STATS_BYTES: u32 = 20;

#[repr(C)]
struct EcReadMem {
    offset: u32,
    bytes: u32,
    buffer: [u8; 256],
}

#[repr(C)]
struct EcCommand {
    version: u32,
    command: u32,
    outsize: u32,
    insize: u32,
    result: u32,
    data: [u8; 4],
}

pub struct EcHardware {
    device: File,
}

impl EcHardware {
    pub fn new() -> Result<Self> {
        let vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor")
            .context("Cannot identify the system manufacturer")?;
        check_device(vendor.trim(), Path::new(DEVICE).exists())?;
        let device = File::open(DEVICE).context("The Framework EC driver is unavailable")?;
        Ok(Self { device })
    }

    fn command(&self, mode: u8, max: u8, min: u8, response_len: usize) -> Result<[u8; 4]> {
        charge_command(mode, max, min, response_len, |cmd| {
            let received = unsafe {
                libc::ioctl(
                    self.device.as_raw_fd(),
                    CROS_EC_DEV_IOCXCMD,
                    cmd as *mut EcCommand,
                )
            };
            if received < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(received)
            }
        })
    }

    fn limits(&self) -> Result<(u8, u8)> {
        let reply = self.command(GET, 0xff, 0xff, 2)?;
        Ok((reply[1], reply[0]))
    }

    pub fn charge_limit(&self) -> Result<u8> {
        Ok(self.limits()?.1)
    }

    pub fn cycle_count(&self) -> Result<u32> {
        let mut memory = EcReadMem {
            offset: BATTERY_FLAGS_OFFSET,
            bytes: BATTERY_STATS_BYTES,
            buffer: [0; 256],
        };
        // The kernel copies the complete readmem structure, so the buffer must
        // match its ABI even though only 20 bytes are requested.
        let received = unsafe {
            libc::ioctl(
                self.device.as_raw_fd(),
                CROS_EC_DEV_IOCRDMEM,
                &mut memory as *mut EcReadMem,
            )
        };
        if received < 0 {
            return Err(io::Error::last_os_error()).context("Cannot read EC battery statistics");
        }
        decode_cycle_count(&memory.buffer, received)
    }

    pub fn set_charge_limit(&self, limit: u8) -> Result<()> {
        if !(25..=100).contains(&limit) {
            bail!("Charge limit must be between 25% and 100%");
        }
        let (min, _) = self.limits()?;
        if min > limit {
            bail!("The EC minimum charge threshold exceeds the requested limit");
        }
        self.command(SET, limit, min, 0)?;
        if self.charge_limit()? != limit {
            bail!("The EC did not retain the requested charge limit");
        }
        Ok(())
    }

    pub fn charge_to_full_once(&self) -> Result<()> {
        // Lilac firmware's GET command clears the volatile override. Do not
        // read the limit after this command, including for verification.
        self.command(OVERRIDE, 0, 0, 0)?;
        Ok(())
    }

    pub fn finish_full_charge(&self) -> Result<()> {
        // Preserve the EC's saved value even if another tool changed it.
        let (min, max) = self.limits()?;
        if max == 0 {
            self.command(DISABLE, 0, 0, 0)?;
        } else if max <= 100 {
            self.command(SET, max, min, 0)?;
        } else {
            bail!("The EC returned an invalid saved charge limit");
        }
        Ok(())
    }

    pub fn battery_state(&self) -> Result<crate::charge::BatteryState> {
        let mut memory = EcReadMem {
            offset: 0x40,
            bytes: 28,
            buffer: [0; 256],
        };
        let received = unsafe {
            libc::ioctl(
                self.device.as_raw_fd(),
                CROS_EC_DEV_IOCRDMEM,
                &mut memory as *mut EcReadMem,
            )
        };
        if received < 0 {
            return Err(io::Error::last_os_error()).context("Cannot read EC battery status");
        }
        decode_battery_state(&memory.buffer, received)
    }
}

fn decode_battery_state(data: &[u8; 256], received: i32) -> Result<crate::charge::BatteryState> {
    if received != 28 || data[12] & 0x20 != 0 {
        bail!("The EC returned unavailable battery status");
    }
    let remaining = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let full = u32::from_le_bytes(data[24..28].try_into().unwrap());
    Ok(crate::charge::BatteryState {
        present: data[12] & 0x02 != 0,
        on_ac: data[12] & 0x01 != 0,
        full: full > 0 && full != u32::MAX && remaining != u32::MAX && remaining >= full,
    })
}

fn decode_cycle_count(data: &[u8; 256], received: i32) -> Result<u32> {
    if received != BATTERY_STATS_BYTES as i32 {
        bail!("The EC returned incomplete battery statistics");
    }
    if data[0] & 0x02 == 0 {
        bail!("The EC reports no battery present");
    }
    // EC_MEMMAP_BATT_CCNT (0x5c) is a little-endian 32-bit count. On this
    // Framework firmware, ACPI's cycle_count loses the upper bytes.
    let cycles = u32::from_le_bytes(data[16..20].try_into().unwrap());
    if cycles == u32::MAX {
        bail!("The EC cycle count is unavailable");
    }
    Ok(cycles)
}

fn charge_command(
    mode: u8,
    max: u8,
    min: u8,
    response_len: usize,
    ioctl: impl FnOnce(&mut EcCommand) -> io::Result<i32>,
) -> Result<[u8; 4]> {
    if response_len > 4 {
        bail!("Charge response buffer is too small");
    }
    let mut cmd = EcCommand {
        version: 0,
        command: CHARGE_LIMIT_CONTROL,
        outsize: 3,
        insize: response_len as u32,
        result: u32::MAX,
        data: [mode, max, min, 0],
    };
    let received = ioctl(&mut cmd).context("EC ioctl failed")?;
    if cmd.result != 0 {
        bail!("The EC rejected the charge command (status {})", cmd.result);
    }
    if received as usize != response_len {
        bail!("The EC returned an unexpected charge response length");
    }
    Ok(cmd.data)
}

fn check_device(vendor: &str, has_cros_ec: bool) -> Result<()> {
    if vendor != "Framework" {
        bail!("This is not a Framework computer");
    }
    if !has_cros_ec {
        bail!("The /dev/cros_ec driver is unavailable");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_state_decodes_ac_presence_and_full_capacity_without_charge_commands() {
        let mut data = [0; 256];
        data[12] = 0x03;
        data[8..12].copy_from_slice(&2470_u32.to_le_bytes());
        data[24..28].copy_from_slice(&3087_u32.to_le_bytes());
        let state = decode_battery_state(&data, 28).unwrap();
        assert!(state.present && state.on_ac && !state.full);
        data[8..12].copy_from_slice(&3087_u32.to_le_bytes());
        assert!(decode_battery_state(&data, 28).unwrap().full);
        data[12] = 0x02;
        assert!(!decode_battery_state(&data, 28).unwrap().on_ac);
        data[24..28].copy_from_slice(&0_u32.to_le_bytes());
        assert!(!decode_battery_state(&data, 28).unwrap().full);
        data[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(!decode_battery_state(&data, 28).unwrap().full);
        data[12] = 0x23;
        assert!(decode_battery_state(&data, 28).is_err());
        assert!(decode_battery_state(&data, 27).is_err());
    }

    #[test]
    fn unsupported_hardware_is_rejected_without_ec_access() {
        assert!(check_device("Other", true).is_err());
        assert!(check_device("Framework", false).is_err());
        assert!(check_device("Framework", true).is_ok());
    }

    #[test]
    fn ioctl_layout_and_charge_command_match_linux_and_framework() {
        assert_eq!(std::mem::size_of::<EcCommand>(), 24);
        assert_eq!(std::mem::offset_of!(EcCommand, data), 20);
        assert_eq!(CROS_EC_DEV_IOCXCMD, 0xc014_ec00);
        assert_eq!(CHARGE_LIMIT_CONTROL, 0x3e03);
        assert_eq!((SET, GET, OVERRIDE), (0x02, 0x08, 0x80));
        assert_eq!(std::mem::size_of::<EcReadMem>(), 264);
        assert_eq!(std::mem::offset_of!(EcReadMem, buffer), 8);
    }

    #[test]
    fn cycle_count_keeps_high_bytes_and_rejects_unavailable_reads() {
        let mut data = [0; 256];
        data[0] = 0x02;
        data[16..20].copy_from_slice(&342_u32.to_le_bytes());
        assert_eq!(decode_cycle_count(&data, 20).unwrap(), 342);
        assert!(decode_cycle_count(&data, 19).is_err());
        data[0] = 0;
        assert!(decode_cycle_count(&data, 20).is_err());
        data[0] = 0x02;
        data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_cycle_count(&data, 20).is_err());
    }

    #[test]
    fn get_and_set_requests_have_expected_bytes_without_ec_access() {
        let get = charge_command(GET, 0xff, 0xff, 2, |cmd| {
            assert_eq!(cmd.command, CHARGE_LIMIT_CONTROL);
            assert_eq!(cmd.outsize, 3);
            assert_eq!(cmd.insize, 2);
            assert_eq!(&cmd.data[..3], &[GET, 0xff, 0xff]);
            cmd.result = 0;
            cmd.data[..2].copy_from_slice(&[80, 0]);
            Ok(2)
        })
        .unwrap();
        assert_eq!(&get[..2], &[80, 0]);
        charge_command(SET, 80, 0, 0, |cmd| {
            assert_eq!(&cmd.data[..3], &[SET, 80, 0]);
            assert_eq!(cmd.insize, 0);
            cmd.result = 0;
            Ok(0)
        })
        .unwrap();
        charge_command(OVERRIDE, 0, 0, 0, |cmd| {
            assert_eq!(&cmd.data[..3], &[OVERRIDE, 0, 0]);
            cmd.result = 0;
            Ok(0)
        })
        .unwrap();
    }

    #[test]
    fn rejects_failed_or_malformed_ec_responses() {
        assert!(charge_command(GET, 0xff, 0xff, 2, |cmd| {
            cmd.result = 3;
            Ok(2)
        })
        .is_err());
        assert!(charge_command(GET, 0xff, 0xff, 2, |cmd| {
            cmd.result = 0;
            Ok(1)
        })
        .is_err());
        assert!(charge_command(GET, 0xff, 0xff, 2, |_| {
            Err(io::Error::from_raw_os_error(libc::EIO))
        })
        .is_err());
    }
}
