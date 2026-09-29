use anyhow::{bail, Context, Result};
use framework_lib::chromium_ec::command::EcCommands;
use framework_lib::chromium_ec::commands::ChargeLimitControlModes;
use framework_lib::chromium_ec::{CrosEc, CrosEcDriver, CrosEcDriverType};
use std::fs;
use std::path::Path;

pub struct EcHardware {
    ec: CrosEc,
}

impl EcHardware {
    pub fn new() -> Result<Self> {
        let vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor")
            .context("Cannot identify the system manufacturer")?;
        check_device(vendor.trim(), Path::new("/dev/cros_ec").exists())?;
        let ec = CrosEc::with(CrosEcDriverType::CrosEc)
            .context("The Framework EC driver is unavailable")?;
        Ok(Self { ec })
    }

    pub fn charge_limit(&self) -> Result<u8> {
        let (_, max) = self
            .ec
            .get_charge_limit()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        Ok(max)
    }

    pub fn set_charge_limit(&self, limit: u8) -> Result<()> {
        if !(25..=100).contains(&limit) {
            bail!("Charge limit must be between 25% and 100%");
        }
        let (min, _) = self
            .ec
            .get_charge_limit()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        if min > limit {
            bail!("The EC minimum charge threshold exceeds the requested limit");
        }
        self.ec
            .set_charge_limit(min, limit)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        if self.charge_limit()? != limit {
            bail!("The EC did not retain the requested charge limit");
        }
        Ok(())
    }

    pub fn charge_to_full_once(&self) -> Result<()> {
        let saved = self.charge_limit()?;
        let request = [ChargeLimitControlModes::Override as u8, 0, 0];
        let reply = self
            .ec
            .send_command(EcCommands::ChargeLimitControl as u16, 0, &request)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        if !reply.is_empty() {
            bail!("The EC returned an unexpected override response");
        }
        if self.charge_limit()? != saved {
            bail!("The EC changed the saved limit during the one-time override");
        }
        Ok(())
    }
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
}
