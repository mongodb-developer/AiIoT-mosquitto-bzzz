use std::{
    sync::atomic::{AtomicU8, Ordering::Relaxed},
    thread,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        adc::{self, attenuation, AdcChannelDriver, AdcDriver, ADC1},
        gpio::{ADCPin, OutputPin},
        modem,
        peripheral::Peripheral,
        peripherals::Peripherals,
        rmt::RmtChannel,
    },
    mqtt::client::{EspMqttClient, MqttClientConfiguration, QoS},
    nvs::EspDefaultNvsPartition,
    wifi::{self, AuthMethod, BlockingWifi, EspWifi},
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

#[toml_cfg::toml_config]
struct Configuration {
    #[default("NotMyWifi")]
    wifi_ssid: &'static str,
    #[default("NotMyPassword")]
    wifi_password: &'static str,
    #[default("mqttserver")]
    mqtt_host: &'static str,
    #[default("")]
    mqtt_user: &'static str,
    #[default("")]
    mqtt_password: &'static str,
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
    let adc = peripherals.adc1;
    let adc_pin = peripherals.pins.gpio0;
    let modem = peripherals.modem;
    thread::scope(|scope| {
        scope.spawn(|| report_status(status, rmt_channel, led_pin));
        thread::Builder::new()
            .stack_size(6144)
            .spawn_scoped(scope, || read_noise_level(status, adc, adc_pin, modem))
            .unwrap();
    });
}

fn read_noise_level<GPIO>(
    status: &AtomicU8,
    adc1: ADC1,
    adc1_pin: GPIO,
    modem: impl Peripheral<P = modem::Modem> + 'static,
) -> !
where
    GPIO: ADCPin<Adc = ADC1>,
{
    const LEN: usize = 5;
    let mut sample_buffer = [0u16; LEN];
    let app_config = CONFIGURATION;
    let mut adc =
        AdcDriver::new(adc1, &adc::config::Config::default()).expect("Unable to initialze ADC1");
    let mut adc_channel: AdcChannelDriver<{ attenuation::DB_11 }, _> =
        AdcChannelDriver::new(adc1_pin).expect("Unable to access ADC1 channel 0");
    let _wifi = match connect_to_wifi(app_config.wifi_ssid, app_config.wifi_password, modem) {
        Ok(wifi) => Some(wifi),
        Err(err) => {
            log::error!("Connect to WiFi: {}", err);
            status.store(DeviceStatus::WifiError as u8, Relaxed);
            None
        }
    };
    const TOPIC: &str = "home/noise sensor/01";
    let mqtt_url = if app_config.mqtt_user.is_empty() || app_config.mqtt_password.is_empty() {
        format!("mqtt://{}/", app_config.mqtt_host)
    } else {
        format!(
            "mqtt://{}:{}@{}/",
            app_config.mqtt_user, app_config.mqtt_password, app_config.mqtt_host
        )
    };

    let mut mqtt_client =
        EspMqttClient::new_cb(&mqtt_url, &MqttClientConfiguration::default(), |_| {
            log::info!("MQTT client callback")
        })
        .expect("Unable to initialize MQTT client");
    let mut mqtt_msg: String;

    loop {
        let mut sum = 0.0f32;
        for i in 0..LEN {
            thread::sleep(Duration::from_millis(10));
            if let Ok(sample) = adc.read(&mut adc_channel) {
                sample_buffer[i] = sample;
                sum += (sample as f32) * (sample as f32);
            } else {
                sample_buffer[i] = 0u16;
            }
        }
        let d_b = 20.0f32 * (sum / LEN as f32).sqrt().log10();
        mqtt_msg = format!("{}", d_b);
        if let Ok(msg_id) = mqtt_client.publish(TOPIC, QoS::AtMostOnce, false, mqtt_msg.as_bytes())
        {
            println!(
                "MSG ID: {}, ADC values: {:?}, sum: {}, and dB: {} ",
                msg_id, sample_buffer, sum, d_b
            );
        } else {
            println!("Unable to send MQTT msg");
        }
    }
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

fn connect_to_wifi(
    ssid: &str,
    passwd: &str,
    modem: impl Peripheral<P = modem::Modem> + 'static,
) -> Result<Box<EspWifi<'static>>> {
    if ssid.is_empty() {
        bail!("No SSID defined");
    }
    let auth_method = if passwd.is_empty() {
        AuthMethod::None
    } else {
        AuthMethod::WPA2Personal
    };
    let sys_loop = EspSystemEventLoop::take().context("Unable to access system event loop.")?;
    let nvs = EspDefaultNvsPartition::take().context("Unable to access default NVS partition")?;
    let mut esp_wifi = EspWifi::new(modem, sys_loop.clone(), Some(nvs))?;
    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sys_loop)?;
    wifi.set_configuration(&wifi::Configuration::Client(wifi::ClientConfiguration {
        ssid: ssid
            .try_into()
            .map_err(|_| anyhow::Error::msg("Failed to use SSID"))?,
        password: passwd
            .try_into()
            .map_err(|_| anyhow::Error::msg("Failed to use password"))?,
        auth_method,
        ..Default::default()
    }))?;
    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    log::info!("DHCP info: {:?}", ip_info);

    Ok(Box::new(esp_wifi))
}
