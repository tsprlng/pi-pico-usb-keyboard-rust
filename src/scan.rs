//! Implements key matrix, scanning for key presses, debouncing, layer selection and other state
//! related to typing. Uses definitions from [crate::keymap], and directly produces packets to be
//! sent out by [crate::usb].

use crate::keymap::*;
use crate::steno::Packet as StenoPacket;
use core::mem::take;
use embassy_rp::{
    gpio::{Input, OutputOpenDrain},
    pwm::{Pwm, SetDutyCycle},
};
use embassy_time::{
    block_for,
    Duration,
};
use usbd_hid::descriptor::KeyboardReport;

#[derive(Clone, Copy, Default)]
pub struct MatrixState {
    left_symbol_key: bool,
    right_symbol_key: bool,
    nav_key: bool,
    function_key: bool,
    emulating_dvorak: bool,
    stenotype: bool,
    awaiting_clear: bool,
}

/// Used to uniquely identify each physical key which can be pressed.
type ScanCode = (u8, u8);

const HELD_KEYS_LIMIT: usize = 16;
const DEFAULT_DEBOUNCE_COUNT: u8 = 5;

const PEDAL_FAKE_SCANCODE: ScanCode = (ROWS as u8, 0);
const MIC_MUTE_KEY: HidKeyCode = 198;  // bodged in here as footswitch function
    // F20 => Xf86AudioMicMute apparently? in theory...
    // ...not that HID code 198 actually results in anything mapping to F20 or to Xf86AudioMicMute.
    // however, 198 does map to keycode 248 in wayland (for whatever reason).
    // so now i'm just using bindcode instead of bindsym in sway, which i guess is fine.

pub struct Matrix<'a> {
    held_keys: HeldKeys,
    steno_packet: StenoPacket,
    state: MatrixState,
    pins: Pins<'a>,
}

pub struct Pins<'a> {
    pub scan_led: Pwm<'a>,
    pub status_led: Pwm<'a>,
    pub rows: [OutputOpenDrain<'a>; ROWS],
    pub columns: [Input<'a>; COLUMNS],
    pub pedal: Input<'a>,
}

trait ConvenientPwm {
    fn on(&mut self);
    fn off(&mut self);
    fn pwm_duty_u16(&mut self, duty: u16);  // TODO is it actually out of a u16?
}
impl ConvenientPwm for Pwm<'_> {
    fn on(&mut self) { self.set_duty_cycle_fully_on().expect("pwm"); }
    fn off(&mut self) { self.set_duty_cycle_fully_off().expect("pwm"); }
    fn pwm_duty_u16(&mut self, duty: u16) { self.set_duty_cycle(duty).expect("pwm"); }
}

impl<'a> Matrix<'a> {
    pub fn new(pins: Pins<'a>) -> Self {
        Matrix {
            held_keys: Default::default(),
            steno_packet: Default::default(),
            state: Default::default(),
            pins,
        }
    }

    fn choose_layer_for_state(&mut self) -> &'static Layer {
        let led = &mut self.pins.status_led;

        if self.state.awaiting_clear {
            led.on()
        } else if self.state.function_key {
            led.pwm_duty_u16(3400)
        } else if self.state.nav_key || (self.state.left_symbol_key && self.state.right_symbol_key) {
            led.pwm_duty_u16(1400)
        } else if self.state.left_symbol_key || self.state.right_symbol_key {
            led.pwm_duty_u16(300)
        } else if self.state.stenotype || self.state.emulating_dvorak {
            led.pwm_duty_u16(5000)
        } else {
            led.off()
        }

        if self.state.function_key {
            &LAYER_FUNCTION
        } else if self.state.nav_key || (self.state.left_symbol_key && self.state.right_symbol_key) {
            &LAYER_NAVIGATION
        } else if self.state.left_symbol_key || self.state.right_symbol_key {
            if self.state.emulating_dvorak { &LAYER_DVORAK_EMU_SYMBOLS } else { &LAYER_SYMBOLS }
        } else if self.state.stenotype {
            &LAYER_STENO
        } else if self.state.emulating_dvorak {
            &LAYER_DVORAK_EMU
        } else {
            &LAYER_NORMAL
        }
    }

    pub fn scan(&mut self) -> (KeyboardReport, StenoPacket, MatrixState) {
        let layer = self.choose_layer_for_state();

        self.held_keys.decrement_holds();

        self.pins.scan_led.pwm_duty_u16(400);
        for (row_idx, row) in self.pins.rows.iter_mut().enumerate() {
            row.set_low();
            block_for(Duration::from_micros(100));
            for (column_idx, column) in self.pins.columns.iter_mut().enumerate() {
                let pressed = column.is_low();
                if pressed {
                    self.held_keys.record_pressed((row_idx as u8, column_idx as u8), layer[row_idx][column_idx]);
                    self.pins.scan_led.pwm_duty_u16(30000);
                }
            }
            row.set_high();
            block_for(Duration::from_micros(100));
        }

        if self.pins.pedal.is_low() {
            self.pins.scan_led.pwm_duty_u16(30000);
            self.held_keys.record_pressed(PEDAL_FAKE_SCANCODE, Thing::RealKey((MIC_MUTE_KEY, 0)));
        }

        let mut report = KeyboardReport::default();
        let mut report_next_keycode_idx = 0;

        self.state.left_symbol_key = false;
        self.state.right_symbol_key = false;
        self.state.nav_key = false;
        self.state.function_key = false;

        for thing in self.held_keys.iter_pressed_things() {
            match thing {
                Thing::RealKey((keycode, mods)) => {
                    if report_next_keycode_idx < 6 {
                        report.modifier |= mods;
                        report.keycodes[report_next_keycode_idx] = *keycode;
                        report_next_keycode_idx += 1;
                    }
                },
                Thing::StenoKey((byte_position, flag)) => {
                    self.state.awaiting_clear = true;
                    self.steno_packet[*byte_position as usize] |= flag;
                },
                Thing::LeftSymbolKey => {
                    self.state.left_symbol_key = true;
                },
                Thing::RightSymbolKey => {
                    self.state.right_symbol_key = true;
                },
                Thing::NavKey => {
                    self.state.nav_key = true;
                },
                Thing::FunctionKey => {
                    self.state.function_key = true;
                },
                Thing::Inactive => {},
                Thing::DvorakToggle => {
                    if ! self.state.awaiting_clear {
                        self.state.emulating_dvorak = !self.state.emulating_dvorak;
                    }
                    self.state.awaiting_clear = true;
                },
                Thing::StenoToggle => {
                    if ! self.state.awaiting_clear {
                        self.state.stenotype = !self.state.stenotype;
                    }
                    self.state.awaiting_clear = true;
                },
            }
        }
        self.pins.scan_led.off();
        if self.state.awaiting_clear {
            if self.held_keys.is_all_released() {
                self.state.awaiting_clear = false;
                return (KeyboardReport::default(), take(&mut self.steno_packet), self.state)
            } else {
                return (KeyboardReport::default(), Default::default(), self.state)
            }
        }
        (report, Default::default(), self.state)
    }
}

/// An array for tracking the currently-held keys.
/// Invariant: Always consists of active [KeyHold]s in order of when they were pressed, followed by
/// only inactive [KeyHold]s (those whose [KeyHold::debounce_count] has reached 0).
#[derive(Default)]
struct HeldKeys ([KeyHold; HELD_KEYS_LIMIT]);

#[derive(Default)]
struct KeyHold {
    debounce_count: u8,
    in_scancode: ScanCode,
    mapping: Thing,
}

impl HeldKeys {
    fn record_pressed(&mut self, code: ScanCode, mapping: Thing) {
        for maybe_key in &mut self.0 {
            if maybe_key.debounce_count > 0 {
                if maybe_key.in_scancode == code {
                    maybe_key.debounce_count = DEFAULT_DEBOUNCE_COUNT;
                    return;
                }
            } else {
                *maybe_key = KeyHold {
                    in_scancode: code,
                    mapping,
                    debounce_count: DEFAULT_DEBOUNCE_COUNT,
                };
                return;
            }
        }
    }

    fn iter_pressed_things(&self) -> impl Iterator<Item = &Thing> {
        self.0.iter().take_while(|key_hold|
            key_hold.debounce_count > 0
        ).map(|key_hold| {
            &key_hold.mapping
        })
    }

    fn is_all_released(&self) -> bool {
        self.0[0].debounce_count == 0
    }

    fn decrement_holds(&mut self) {
        'each_position: for key_idx in 0..HELD_KEYS_LIMIT {
            'each_rotation: loop {
                let key = &mut self.0[key_idx];
                if key.debounce_count > 0 {
                    key.debounce_count -= 1;
                    if key.debounce_count == 0 {
                        self.0[key_idx..].rotate_left(1);
                            // move to end of array to preserve invariant.
                            // now next key has taken its place at current index, so look again:
                        continue 'each_rotation;
                    } else {
                        continue 'each_position;
                    }
                } else {
                    break 'each_position;
                }
            }
        }
    }
}
