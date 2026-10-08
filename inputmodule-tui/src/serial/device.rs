use inputmodule_host::protocol::{FRAMEWORK_VID, LED_MATRIX_PID};
use serialport::{SerialPortInfo, SerialPortType};
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub port: String,
    pub serial_number: Option<String>,
    pub product: Option<String>,
}

pub fn list_devices() -> io::Result<Vec<DeviceInfo>> {
    let ports = serialport::available_ports().map_err(io::Error::from)?;
    Ok(compatible_devices(&ports))
}

fn compatible_devices(ports: &[SerialPortInfo]) -> Vec<DeviceInfo> {
    let mut devices: Vec<_> = ports
        .iter()
        .filter_map(|port| {
            let SerialPortType::UsbPort(info) = &port.port_type else {
                return None;
            };
            if info.vid != FRAMEWORK_VID || info.pid != LED_MATRIX_PID {
                return None;
            }
            #[cfg(target_os = "macos")]
            if port.port_name.starts_with("/dev/tty.") {
                return None;
            }
            Some(DeviceInfo {
                port: port.port_name.clone(),
                serial_number: info.serial_number.clone(),
                product: info.product.clone(),
            })
        })
        .collect();
    devices.sort_by(|a, b| a.port.cmp(&b.port));
    devices
}

pub fn find_device(requested: Option<&str>) -> io::Result<Option<String>> {
    if let Some(port) = requested {
        return Ok(Some(port.to_owned()));
    }
    select_device(list_devices()?)
}

fn select_device(devices: Vec<DeviceInfo>) -> io::Result<Option<String>> {
    match devices.as_slice() {
        [] => Ok(None),
        [device] => Ok(Some(device.port.clone())),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Multiple LED Matrices found ({}); choose --serial-dev",
                devices
                    .iter()
                    .map(|d| d.port.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguity_is_explicit_and_manual_paths_need_no_enumeration() {
        let d = |p: &str| DeviceInfo {
            port: p.into(),
            serial_number: None,
            product: None,
        };
        assert!(select_device(vec![d("a"), d("b")]).is_err());
        assert_eq!(select_device(vec![d("a")]).unwrap(), Some("a".into()));
        assert_eq!(
            find_device(Some("/dev/serial/by-id/example")).unwrap(),
            Some("/dev/serial/by-id/example".into())
        );
    }
}
