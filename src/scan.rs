//! Implements key matrix, scanning for key presses, debouncing, layer selection and other state
//! related to typing. Uses definitions from [crate::keymap], and directly produces packets to be
//! sent out by [crate::usb].

use crate::keymap::*;
use crate::steno::Packet as StenoPacket;
use embassy_rp::{
    gpio::{Input, OutputOpenDrain},
    pwm::{Pwm, SetDutyCycle},
};
use embassy_time::{
    block_for,
    Duration,
};
use usbd_hid::descriptor::KeyboardReport;

type ScanCode = (u8, u8);

struct KeyHold {
    in_scancode: ScanCode,
    mapping: Thing,
    debounce_count: u8,
}

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

const HELD_KEYS_LIMIT: usize = 16;
const DEFAULT_DEBOUNCE_COUNT: u8 = 5;

type HidKeyCode = u8;
const MIC_MUTE_KEY: HidKeyCode = 198;  // bodged in here as footswitch function
    // F20 => Xf86AudioMicMute apparently? in theory...
    // ...not that HID code 198 actually results in anything mapping to F20 or to Xf86AudioMicMute.
    // however, 198 does map to keycode 248 in wayland (for whatever reason).
    // so now i'm just using bindcode instead of bindsym in sway, which i guess is fine.

pub struct Matrix<'a> {
    held_keys: [Option<KeyHold>; HELD_KEYS_LIMIT],
    steno_packet: StenoPacket,
    state: MatrixState,
    led_pin: Pwm<'a>,
    led_pin_b: Pwm<'a>,
    row_pins: [OutputOpenDrain<'a>; ROWS],
    column_pins: [Input<'a>; COLUMNS],
    pedal_pin: Input<'a>,
}

fn add_pressed(keys: &mut [Option<KeyHold>], code: ScanCode, mapping: Thing) {
    for maybe_key in keys {
        if let Some(key) = maybe_key {
            if key.in_scancode == code {
                key.debounce_count = DEFAULT_DEBOUNCE_COUNT;
                return;
            }
        } else {
            *maybe_key = Some(KeyHold {
                in_scancode: code,
                mapping,
                debounce_count: DEFAULT_DEBOUNCE_COUNT,
            });
            return;
        }
    }
}

impl<'a> Matrix<'a> {
    pub fn new(led_pin: Pwm<'a>, led_pin_b: Pwm<'a>, row_pins: [OutputOpenDrain<'a>; ROWS], column_pins: [Input<'a>; COLUMNS], pedal_pin: Input<'a>) -> Self {
        Matrix {
            held_keys: Default::default(),
            steno_packet: Default::default(),
            state: Default::default(),
            row_pins,
            column_pins,
            led_pin,
            led_pin_b,
            pedal_pin,
        }
    }

    fn choose_layer_for_state(&mut self) -> &'static Layer {
        if self.state.awaiting_clear {
            self.led_pin_b.set_duty_cycle_fully_on().expect("pwm");
        } else if self.state.function_key {
            self.led_pin_b.set_duty_cycle(3400).expect("pwm");
        } else if self.state.nav_key || (self.state.left_symbol_key && self.state.right_symbol_key) {
            self.led_pin_b.set_duty_cycle(1400).expect("pwm");
        } else if self.state.left_symbol_key || self.state.right_symbol_key {
            self.led_pin_b.set_duty_cycle(300).expect("pwm");
        } else if self.state.stenotype || self.state.emulating_dvorak {
            self.led_pin_b.set_duty_cycle(5000).expect("pwm");
        } else {
            self.led_pin_b.set_duty_cycle_fully_off().expect("pwm");
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

    fn prune_held(&mut self) {
        for key_idx in 0..HELD_KEYS_LIMIT {
            if let Some(key) = &self.held_keys[key_idx] {
                if key.debounce_count == 0 {
                    self.held_keys[key_idx..].rotate_left(1);
                    self.held_keys[HELD_KEYS_LIMIT - 1] = None;
                }
            } else {
                break;
            }
        }
    }

    pub fn scan(&mut self) -> (KeyboardReport, StenoPacket, MatrixState) {
        let layer = self.choose_layer_for_state();

        for held_key in self.held_keys.iter_mut().flatten() {
            if held_key.debounce_count > 0 {
                held_key.debounce_count -= 1;
            }
        }
        self.prune_held();

        self.led_pin.set_duty_cycle(400).expect("pwm");
        for (row_idx, row) in self.row_pins.iter_mut().enumerate() {
            row.set_low();
            block_for(Duration::from_micros(100));
            for (column_idx, column) in self.column_pins.iter_mut().enumerate() {
                let pressed = column.is_low();
                if pressed {
                    add_pressed(&mut self.held_keys, (row_idx as u8, column_idx as u8), layer[row_idx][column_idx]);
                    self.led_pin.set_duty_cycle(30000).expect("pwm");
                }
            }
            row.set_high();
            block_for(Duration::from_micros(100));
        }

        if self.pedal_pin.is_low() {
            self.led_pin.set_duty_cycle(30000).expect("pwm");
            add_pressed(&mut self.held_keys, (8,0), Thing::RealKey((MIC_MUTE_KEY, 0)));
        }

        let mut report = KeyboardReport::default();

        self.state.left_symbol_key = false;
        self.state.right_symbol_key = false;
        self.state.nav_key = false;
        self.state.function_key = false;
        let mut held_idx = 0;
        for maybe_key in &self.held_keys {
            if let Some(key_hold) = maybe_key {
                match key_hold.mapping {
                    Thing::RealKey((keycode, mods)) => {
                        report.modifier |= mods;
                        report.keycodes[held_idx] = keycode;
                        held_idx += 1;
                        if held_idx >= 6 {
                            break;
                        }
                    },
                    Thing::StenoKey((byte_position, flag)) => {
                        self.state.awaiting_clear = true;
                        self.steno_packet[byte_position as usize] |= flag;
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
            } else {
                break;
            }
        }
        self.led_pin.set_duty_cycle_fully_off().expect("pwm");
        if self.state.awaiting_clear {
            if self.held_keys[0].is_none() {
                self.state.awaiting_clear = false;
                let steno_packet = self.steno_packet;
                self.steno_packet = Default::default();
                return (KeyboardReport::default(), steno_packet, self.state)
            } else {
                return (KeyboardReport::default(), Default::default(), self.state)
            }
        }
        (report, Default::default(), self.state)
    }
}
