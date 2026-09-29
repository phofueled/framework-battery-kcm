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
// _IOWR(0xEC, 0, struct cros_ec_command): the kernel header is five u32s.
const CROS_EC_DEV_IOCXCMD: libc::c_ulong = 0xc014_ec00;

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
        let saved = self.charge_limit()?;
        self.command(OVERRIDE, 0, 0, 0)?;
        if self.charge_limit()? != saved {
            bail!("The EC changed the saved limit during the one-time override");
        }
        Ok(())
    }
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
