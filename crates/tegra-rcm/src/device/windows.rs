use std::thread;
use std::time::Duration;

use libusbk::{DeviceHandle, DeviceList};

use super::DeviceRaw;
use super::{Device, SwitchDeviceRaw};
use crate::error::WindowsDriver;
use crate::Result;

/// A connected and init switch device connection
#[derive(Debug)]
pub struct SwitchDevice {
    device: DeviceHandle,
    claimed: bool,
}

impl Device for SwitchDevice {
    /// Init the device
    fn init(&mut self) -> Result<()> {
        if !self.claimed {
            self.claimed = true;
        }
        self.validate()
    }

    /// Read from the device into the buffer
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let amount = self.device.read_pipe(0x81, buf)?;
        Ok(amount as usize)
    }

    /// Write to the device from the buffer
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let amount = self.device.write_pipe(0x01, buf)?;
        Ok(amount as usize)
    }

    fn validate(&self) -> Result<()> {
        match WindowsDriver::from(self.device().driver_id()) {
            WindowsDriver::LibUsbK => Ok(()),
            driver => Err(crate::Error::WindowsWrongDriver(driver)),
        }
    }
}

impl SwitchDevice {
    pub fn with_device_handle(device: DeviceHandle) -> Self {
        Self {
            device,
            claimed: false,
        }
    }

    pub fn device(&self) -> &DeviceHandle {
        &self.device
    }
}

impl SwitchDeviceRaw {
    fn open_device_with_vid_pid(vid: u16, pid: u16) -> Result<DeviceHandle> {
        let devices = DeviceList::new()?;
        let device = devices.find_with_vid_and_pid(vid as i32, pid as i32);
        if let Ok(dev) = device {
            let handle = dev.open()?;
            return Ok(handle);
        }

        Err(crate::Error::SwitchNotFound)
    }
}

impl DeviceRaw for SwitchDeviceRaw {
    /// Tries to connect to the device and open and interface
    fn find_device(self, wait: bool) -> Result<SwitchDevice> {
        let mut device = Self::open_device_with_vid_pid(self.vid, self.pid);
        while wait && device.is_err() {
            thread::sleep(Duration::from_secs(1));
            device = Self::open_device_with_vid_pid(self.vid, self.pid);
        }

        if let Err(ref err) = device {
            if *err == crate::Error::SwitchNotFound {
                return Err(crate::Error::SwitchNotFound);
            }
        }

        let device = device?;

        Ok(SwitchDevice::with_device_handle(device))
    }
}
