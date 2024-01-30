use std::{
    sync::atomic::{AtomicU8, Ordering::Relaxed},
    thread,
    time::Duration,
};

use esp_idf_svc::hal::{
    gpio::OutputPin, peripheral::Peripheral, peripherals::Peripherals, rmt::RmtChannel,
};
use ws2812_esp32_rmt_driver::{
    driver::color::{LedPixelColor, LedPixelColorGrb24},
    Ws2812Esp32RmtDriver,
};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
enum DeviceStatus {
    Ok,
    WifiError,
    MqttError,
}

impl DeviceStatus {
    fn light_sequence(&self) -> Vec<ColorStep> {
        match self {
            DeviceStatus::Ok => vec![ColorStep::new(0, 255, 0, 500), ColorStep::new(0, 0, 0, 500)],
            DeviceStatus::WifiError => {
                vec![ColorStep::new(255, 0, 0, 200), ColorStep::new(0, 0, 0, 100)]
            }
            DeviceStatus::MqttError => vec![
                ColorStep::new(255, 0, 255, 100),
                ColorStep::new(0, 0, 0, 300),
            ],
        }
    }
}

impl TryFrom<u8> for DeviceStatus {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0u8 => Ok(DeviceStatus::Ok),
            1u8 => Ok(DeviceStatus::WifiError),
            2u8 => Ok(DeviceStatus::MqttError),
            _ => Err("Unknown status"),
        }
    }
}

struct ColorStep {
    red: u8,
    green: u8,
    blue: u8,
    duration: u64,
}

impl ColorStep {
    fn new(red: u8, green: u8, blue: u8, duration: u64) -> Self {
        ColorStep {
            red,
            green,
            blue,
            duration,
        }
    }
}

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let status = &AtomicU8::new(0u8);
    let peripherals = Peripherals::take().expect("Unable to access device peripherals");
    let rmt_channel = peripherals.rmt.channel0;
    let led_pin = peripherals.pins.gpio8;
    thread::scope(|scope| {
        scope.spawn(|| report_status(status, rmt_channel, led_pin));
        scope.spawn(|| change_status(status));
    });
}

fn report_status(
    status: &AtomicU8,
    rmt_channel: impl Peripheral<P = impl RmtChannel>,
    led_pin: impl Peripheral<P = impl OutputPin>,
) -> ! {
    let mut neopixel =
        Ws2812Esp32RmtDriver::new(rmt_channel, led_pin).expect("Unable to talk to ws2812");
    let mut prev_status = DeviceStatus::WifiError; // Anything but Ok
    let mut sequence: Vec<ColorStep> = vec![];
    loop {
        if let Ok(status) = DeviceStatus::try_from(status.load(Relaxed)) {
            if status != prev_status {
                prev_status = status;
                sequence = status.light_sequence();
            }
            for step in sequence.iter() {
                let color = LedPixelColorGrb24::new_with_rgb(step.red, step.green, step.blue);
                neopixel
                    .write_blocking(color.as_ref().iter().cloned())
                    .expect("Error writing to neopixel");
                thread::sleep(Duration::from_millis(step.duration));
            }
        }
    }
}

fn change_status(status: &AtomicU8) -> ! {
    loop {
        thread::sleep(Duration::from_secs(10));
        if let Ok(current) = DeviceStatus::try_from(status.load(Relaxed)) {
            match current {
                DeviceStatus::Ok => status.store(DeviceStatus::WifiError as u8, Relaxed),
                DeviceStatus::WifiError => status.store(DeviceStatus::MqttError as u8, Relaxed),
                DeviceStatus::MqttError => status.store(DeviceStatus::Ok as u8, Relaxed),
            }
        }
    }
}
