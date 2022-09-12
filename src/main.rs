use embedded_hal::digital::v2::{InputPin, OutputPin, ToggleableOutputPin};

use embedded_svc::wifi::{AccessPointConfiguration, Configuration, Wifi};
use esp_idf_svc::{
    espnow::{EspNowClient, PeerInfo, BROADCAST},
    netif::EspNetifStack,
    nvs::EspDefaultNvs,
    sysloop::EspSysLoopStack,
    wifi::EspWifi,
};
use esp_idf_sys as _;
use futures_micro::yield_once as yield_now;

use std::{
    fmt::Debug,
    sync::Arc,
    time::{Duration, Instant},
};

async fn sleep(dur: Duration) -> usize {
    let when = Instant::now() + dur;
    let mut count = 0;
    loop {
        if Instant::now() >= when {
            break count;
        } else {
            count += 1;
            yield_now().await;
        }
    }
}

async fn wait_low<E: Debug>(pin: &mut impl InputPin<Error = E>) {
    loop {
        if pin.is_low().unwrap() {
            break;
        } else {
            sleep(Duration::from_millis(1)).await;
        }
    }
}

async fn wait_high<E: Debug>(pin: &mut impl InputPin<Error = E>) {
    loop {
        if pin.is_high().unwrap() {
            break;
        } else {
            sleep(Duration::from_millis(1)).await;
        }
    }
}

fn main() {
    esp_idf_sys::link_patches();

    let hal = esp_idf_hal::peripherals::Peripherals::take().unwrap();

    let netif_stack = Arc::new(EspNetifStack::new().unwrap());
    let sys_loop_stack = Arc::new(EspSysLoopStack::new().unwrap());
    let default_nvs = Arc::new(EspDefaultNvs::new().unwrap());

    let mut wifi = EspWifi::new(netif_stack, sys_loop_stack, default_nvs).unwrap();
    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: "wifi".into(),
        ..Default::default()
    }))
    .unwrap();

    let (tx_now, rx_now) = async_channel::bounded::<()>(10);
    let (tx_led, rx_led) = async_channel::bounded::<()>(10);

    let espnow = EspNowClient::new().unwrap();

    espnow
        .add_peer(PeerInfo {
            peer_addr: BROADCAST,
            ifidx: 1,
            ..Default::default()
        })
        .unwrap();

    espnow
        .register_recv_cb(move |_addr, _dat| {
            tx_now.try_send(());
        })
        .unwrap();

    let mut pin9 = hal.pins.gpio9.into_input().unwrap();
    let mut pin1 = hal.pins.gpio3.into_output().unwrap();
    let mut pin2 = hal.pins.gpio4.into_output().unwrap();

    let task1 = async {
        loop {
            wait_low(&mut pin9).await;
            tx_led.try_send(());
            wait_high(&mut pin9).await;
            tx_led.try_send(());
            espnow.send(BROADCAST, &[123]).expect("Send error");
        }
    };

    let task2 = async {
        loop {
            if let Ok(_item) = rx_now.recv().await {
                pin1.set_high().unwrap();
                sleep(Duration::from_secs(1)).await;
                pin1.set_low().unwrap();
            }
        }
    };

    let task3 = async {
        loop {
            if let Ok(_) = rx_led.recv().await {
                pin2.toggle().unwrap();
            }
        }
    };
    let sleep_task = async {
        loop {
            std::thread::sleep(Duration::from_millis(1));
            yield_now().await
        }
    };

    spin_on::spin_on(futures_micro::zip!(sleep_task, task1, task2, task3));
}
