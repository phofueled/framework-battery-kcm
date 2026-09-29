mod hardware;
mod schedule;
mod storage;

use anyhow::{bail, Result};
use hardware::EcHardware;
use schedule::Schedule;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zbus::connection::Builder;
use zbus::message::Header;
use zbus::{fdo, interface, Connection};
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

const BUS_NAME: &str = "org.frameworkbattery.Control1";
const OBJECT_PATH: &str = "/org/frameworkbattery/Control1";
const ACTION: &str = "org.frameworkbattery.modify";

#[derive(Default)]
struct Activity {
    last: Option<Instant>,
    active_calls: usize,
}

struct BatteryService {
    activity: Arc<Mutex<Activity>>,
}

struct CallGuard(Arc<Mutex<Activity>>);

impl BatteryService {
    fn begin(&self) -> CallGuard {
        let mut activity = self.activity.lock().unwrap();
        activity.last = Some(Instant::now());
        activity.active_calls += 1;
        CallGuard(self.activity.clone())
    }
}

impl Drop for CallGuard {
    fn drop(&mut self) {
        let mut activity = self.0.lock().unwrap();
        activity.last = Some(Instant::now());
        activity.active_calls -= 1;
    }
}

fn dbus_error(error: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::Failed(error.to_string())
}

async fn is_authorized(conn: &Connection, header: &Header<'_>) -> fdo::Result<bool> {
    let subject = Subject::new_for_message_header(header).map_err(dbus_error)?;
    let proxy = AuthorityProxy::new(conn).await.map_err(dbus_error)?;
    let details: HashMap<&str, &str> = HashMap::new();
    let result = proxy
        .check_authorization(
            &subject,
            ACTION,
            &details,
            CheckAuthorizationFlags::AllowUserInteraction.into(),
            "",
        )
        .await
        .map_err(dbus_error)?;
    Ok(result.is_authorized)
}

fn authorized_write(authorized: bool, operation: impl FnOnce() -> Result<()>) -> fdo::Result<()> {
    if !authorized {
        return Err(fdo::Error::AccessDenied("Authorization was denied".into()));
    }
    operation().map_err(dbus_error)
}

#[interface(name = "org.frameworkbattery.Control1")]
impl BatteryService {
    async fn get_charge_limit(&self) -> fdo::Result<u32> {
        let _guard = self.begin();
        EcHardware::new()
            .map_err(dbus_error)?
            .charge_limit()
            .map(u32::from)
            .map_err(dbus_error)
    }

    async fn set_charge_limit(
        &self,
        limit: u32,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let _guard = self.begin();
        let limit = u8::try_from(limit).map_err(dbus_error)?;
        if !(25..=100).contains(&limit) {
            return Err(fdo::Error::InvalidArgs("Limit must be 25–100%".into()));
        }
        let authorized = is_authorized(conn, &header).await?;
        authorized_write(authorized, || EcHardware::new()?.set_charge_limit(limit))
    }

    async fn charge_to_full_once(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let _guard = self.begin();
        if !storage::override_confirmed() {
            return Err(fdo::Error::NotSupported(
                "One-time charge override has not been verified on this device".into(),
            ));
        }
        let authorized = is_authorized(conn, &header).await?;
        authorized_write(authorized, || EcHardware::new()?.charge_to_full_once())
    }

    async fn get_override_available(&self) -> fdo::Result<bool> {
        let _guard = self.begin();
        Ok(storage::override_confirmed())
    }

    async fn get_schedule(&self) -> fdo::Result<String> {
        let _guard = self.begin();
        let schedule = storage::load().map_err(dbus_error)?;
        serde_json::to_string(&schedule).map_err(dbus_error)
    }

    async fn set_schedule(
        &self,
        schedule_json: &str,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let _guard = self.begin();
        if schedule_json.len() > 16_384 {
            return Err(fdo::Error::InvalidArgs("Schedule is too large".into()));
        }
        let schedule: Schedule = serde_json::from_str(schedule_json)
            .map_err(|error| fdo::Error::InvalidArgs(error.to_string()))?;
        schedule
            .validate()
            .map_err(|error| fdo::Error::InvalidArgs(error.to_string()))?;
        let authorized = is_authorized(conn, &header).await?;
        authorized_write(authorized, || storage::save(&schedule))
    }
}

async fn serve() -> Result<()> {
    let activity = Arc::new(Mutex::new(Activity {
        last: Some(Instant::now()),
        active_calls: 0,
    }));
    let service = BatteryService {
        activity: activity.clone(),
    };
    let _connection = Builder::system()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()
        .await?;

    loop {
        async_io::Timer::after(Duration::from_secs(5)).await;
        let state = activity.lock().unwrap();
        if state.active_calls == 0
            && state
                .last
                .is_some_and(|last| last.elapsed() >= Duration::from_secs(60))
        {
            break;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("The Framework battery helper must run as root");
    }
    match std::env::args().nth(1).as_deref() {
        Some("serve") => futures_lite::future::block_on(serve()),
        Some("apply-schedule") => storage::apply_saved(),
        Some("verify-override") => storage::verify_override(),
        Some("self-test") => storage::hardware_self_test(),
        Some("read-limit") => {
            println!("{}", EcHardware::new()?.charge_limit()?);
            Ok(())
        }
        _ => bail!("Usage: framework-battery-helper serve|apply-schedule|verify-override|self-test|read-limit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn denied_write_never_runs_the_operation() {
        let ran = AtomicBool::new(false);
        let result = authorized_write(false, || {
            ran.store(true, Ordering::SeqCst);
            Ok(())
        });
        assert!(matches!(result, Err(fdo::Error::AccessDenied(_))));
        assert!(!ran.load(Ordering::SeqCst));
    }
}
