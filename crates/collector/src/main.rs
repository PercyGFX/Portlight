//! Portlight Collector: a small Windows service that reads Windows' live
//! network events and passes them to the Portlight window.
//!
//! Reading those events needs administrator rights. Running this part as a
//! service lets the window itself stay an ordinary program. The service only
//! collects and forwards; every rule about apps lives in the window.
//!
//! Usage (all but `service` need an administrator command prompt):
//!   portlight-service install     copy to Program Files, register and start
//!   portlight-service uninstall   stop, unregister and delete
//!   portlight-service console     run in this console, printing what it sees
//!   portlight-service service     what Windows runs; not for people

mod collect;
mod setup;

use std::ffi::OsString;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use portlight_platform::service::SERVICE_NAME;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

fn main() -> ExitCode {
    let command = std::env::args().nth(1).unwrap_or_default();
    match command.as_str() {
        "service" => {
            if let Err(error) = service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
                collect::log(&format!("could not start as a service: {error}"));
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        "console" => {
            let stop = Arc::new(AtomicBool::new(false));
            match collect::run(&stop, true) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("portlight-service: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        "install" => setup::install(),
        "uninstall" => setup::uninstall(),
        _ => {
            eprintln!("Portlight Collector {}", env!("CARGO_PKG_VERSION"));
            eprintln!("Usage: portlight-service install | uninstall | console");
            ExitCode::from(setup::USAGE)
        }
    }
}

define_windows_service!(ffi_service_main, service_main);

fn service_main(_arguments: Vec<OsString>) {
    let stop = Arc::new(AtomicBool::new(false));
    let on_control = {
        let stop = Arc::clone(&stop);
        move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                stop.store(true, Ordering::SeqCst);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let status = match service_control_handler::register(SERVICE_NAME, on_control) {
        Ok(status) => status,
        Err(error) => {
            collect::log(&format!("could not register with Windows: {error}"));
            return;
        }
    };

    let report = |state: ServiceState, exit: u32| {
        let _ = status.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {
                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: ServiceExitCode::Win32(exit),
            checkpoint: 0,
            wait_hint: Duration::from_secs(5),
            process_id: None,
        });
    };

    report(ServiceState::Running, 0);
    let result = collect::run(&stop, false);
    if let Err(error) = &result {
        collect::log(&format!("stopped: {error}"));
    }
    // A non-zero exit code tells Windows to restart the service (see setup.rs).
    report(ServiceState::Stopped, if result.is_ok() { 0 } else { 1 });
}
