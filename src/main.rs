use std::{thread, time::Duration};

use esp_idf_svc::hal::peripherals::Peripherals;
use ws2812_esp32_rmt_driver::{
    driver::color::{LedPixelColor, LedPixelColorGrb24},
    Ws2812Esp32RmtDriver,
};

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let peripherals = Peripherals::take().expect("Unable to access device peripherals");
    let led_pin = peripherals.pins.gpio8;
    let rmt_channel = peripherals.rmt.channel0;
    let mut neopixel =
        Ws2812Esp32RmtDriver::new(rmt_channel, led_pin).expect("Unable to talk to ws2812");
    let color_1 = LedPixelColorGrb24::new_with_rgb(255, 255, 0);
    let color_2 = LedPixelColorGrb24::new_with_rgb(255, 0, 255);

    loop {
        neopixel
            .write_blocking(color_1.as_ref().iter().cloned())
            .expect("Error writing to neopixel");
        thread::sleep(Duration::from_millis(500));
        neopixel
            .write_blocking(color_2.as_ref().iter().cloned())
            .expect("Error writing to neopixel");
        thread::sleep(Duration::from_millis(500));
    }
}
