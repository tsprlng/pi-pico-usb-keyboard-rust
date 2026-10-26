//! Firmware for a custom USB keyboard based on the Raspberry Pi Pico, using the [embassy_rp]
//! framework.

#![no_main]
#![no_std]

mod scan;
mod keymap;
mod usb;
mod steno;

/// Useful constants (such as keycodes) extracted from the otherwise-unrelated [rmk](https://github.com/HaoboGu/rmk/) project.
mod rmk;

use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Input, OutputOpenDrain, Level, Pull},
    pwm::Pwm,
};
use embassy_sync::channel::Channel;

use panic_reset as _;

macro_rules! row_pins {
    ($dev:ident; $($pin:ident),*) => {[ $(OutputOpenDrain::new($dev.$pin, Level::High)),* ]}
}
macro_rules! column_pins {
    ($dev:ident; $($pin:ident),*) => {[ $(Input::new($dev.$pin, Pull::Up)),* ]}
}

/// Channel for [scan] to send keyboard updates to [usb], and ultimately to the host.
pub(crate) static UPDATES_CHANNEL: Channel<RawMutex, Update, 1> = Channel::new();
type RawMutex = embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
type Update = (usbd_hid::descriptor::KeyboardReport, steno::Packet);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut init_config = embassy_rp::config::Config::default();
    let mut init_clocks = embassy_rp::clocks::ClockConfig::crystal(12_000_000);
    init_clocks.adc_clk = None;
    init_clocks.rtc_clk = None;  // ..etc?
    init_config.clocks = init_clocks;
    let p = embassy_rp::init(init_config);

    let led_pin_onboard = Pwm::new_output_b(p.PWM_SLICE4, p.PIN_25, Default::default());
    let led_pin_front = Pwm::new_output_a(p.PWM_SLICE3, p.PIN_22, Default::default());

    let pedal_pin = Input::new(p.PIN_2, Pull::Up);

    let row_pins: [OutputOpenDrain; keymap::ROWS] = row_pins!(p;
        PIN_10, PIN_11, PIN_12, PIN_13, PIN_21, PIN_20, PIN_19, PIN_18
    );
    let mut column_pins: [Input; keymap::COLUMNS] = column_pins!(p;
        PIN_17, PIN_8, PIN_16, PIN_15, PIN_14, PIN_9
    );
    for pin in &mut column_pins {
        pin.set_schmitt(true);
    }

    let matrix = scan::Matrix::new(scan::Pins {
        scan_led: led_pin_onboard,
        status_led: led_pin_front,
        rows: row_pins,
        columns: column_pins,
        pedal: pedal_pin,
    });
    spawner.spawn(run_matrix(matrix)).expect("spawn matrix");

    let usb_driver = embassy_rp::usb::Driver::new(p.USB, usb::Irqs);
    let (usb_device, hid, cdc) = usb::get_device(usb_driver);
    spawner.spawn(usb::run(usb_device, hid, cdc)).expect("spawn usb");
}

#[embassy_executor::task]
async fn run_matrix(mut matrix: scan::Matrix<'static>) {
    let mut blanks = 0;
    loop {
        let (hid_report, steno_packet, _state) = matrix.scan();
        UPDATES_CHANNEL.send((hid_report, steno_packet)).await;
        if hid_report == usbd_hid::descriptor::KeyboardReport::default() && steno_packet.iter().all(|s| *s==0) {
            blanks += 1;
        }
        if blanks > 300 {
            matrix.pause().await;
            blanks = 0;
        }
    }
}
