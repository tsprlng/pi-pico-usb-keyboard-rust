//! Defines key functions (here called [Thing]s) and the different layers of mapping from physical
//! keys to these [Thing]s.
//!
//! Intimately related to [crate::scan], which uses these definitions to actually scan for and
//! interpret physical key presses.

use crate::rmk::keycode::KeyCode;
use crate::rmk::keycode::KeyCode::*;
use crate::steno::{KeyCode as StenoKeyCode, PacketCode as StenoPacketCode};
use core::marker::Copy;

type HidKeyCode = u8;
type Modifiers = u8;
type Key = (HidKeyCode, Modifiers);

/// A Thing which a keypress should Do
#[derive(Clone, Copy, Debug)]
pub enum Thing {
    RealKey(Key),
    StenoKey(StenoPacketCode),
    LeftSymbolKey,
    RightSymbolKey,
    NavKey,
    FunctionKey,
    DvorakToggle,
    StenoToggle,
    Inactive,
}

/// How many physical rows there are
pub const ROWS: usize = 8;
/// How many physical columns there are
pub const COLUMNS: usize = 6;

/// Array of [Thing]s that a row of keys do
pub type Row = [Thing; COLUMNS];
/// 2D Array of [Thing]s that the whole set of keys do
pub type Layer = [Row; ROWS];

/// Maps a modifier [KeyCode] to the equivalent flag bit for the USB HID modifier byte, or returns
/// 0 for any non-modifier [KeyCode].
const fn modifier_key_bit_repr(code: KeyCode) -> u8 {
    match code {
        LCtrl => 0x01,
        LShift => 0x02,
        LAlt => 0x04,
        LGui => 0x08,
        RCtrl => 0x10,
        RShift => 0x20,
        RAlt => 0x40,
        RGui => 0x80,
        _ => 0,
    }
}

/// Flip a row definition around, as on my keyboards the left rows have their columns pinned in the
/// opposite direction from the right rows.
///
/// Conventionally I'm using column 0 to mean "near the controller", and 5 "near the sides"
const fn rev<A: Copy>(r: [A; COLUMNS]) -> [A; COLUMNS] {
    [r[5], r[4], r[3], r[2], r[1], r[0]]
}

/// Translate a [KeyCode] into a valid [Thing]
const fn k(k: KeyCode) -> Thing {
    let maybe_modifier_key = modifier_key_bit_repr(k);
    if maybe_modifier_key != 0 {
        return Thing::RealKey((0, maybe_modifier_key));
    }

    let k = k as u16;
    assert!(k > 0 && k <= 255);
    Thing::RealKey((k as u8, 0))
}

/// Translate a [KeyCode] into a valid [Thing], that also holds left-shift while typing that keycode
const fn shift(kc: KeyCode) -> Thing {
    let Thing::RealKey((code, mods)) = k(kc) else { panic!("shift() with abnormal keycode") };
    Thing::RealKey((code, mods | modifier_key_bit_repr(LShift)))
}

const DFA: Thing = Thing::Inactive;

/// Regular layer for typing words
pub const LAYER_NORMAL: Layer = [
    rev([k(Tab), k(Q), k(W), k(E), k(R), k(T)]),
    rev([k(Backspace), k(A), k(S), k(D), k(F), k(G)]),
    rev([k(Escape), k(Z), k(X), k(C), k(V), k(B)]),
    rev([k(LShift), Thing::FunctionKey, k(RGui), k(LAlt), k(LCtrl), Thing::LeftSymbolKey]),
        [k(Y), k(U), k(I), k(O), k(P), k(LeftBracket)],
        [k(H), k(J), k(K), k(L), k(Semicolon), k(Quote)],
        [k(N), k(M), k(Comma), k(Dot), k(Slash), Thing::NavKey],
        [Thing::RightSymbolKey, k(Space), k(LGui), k(RCtrl), k(RAlt), k(RShift)],
];

/// Emulates dvorak layout on other people's computers configured for qwerty
pub const LAYER_DVORAK_EMU: Layer = [
    rev([k(Tab), k(Quote), k(Comma), k(Dot), k(P), k(Y)]),
    rev([k(Backspace), k(A), k(O), k(E), k(U), k(I)]),
    rev([k(Escape), k(Semicolon), k(Q), k(J), k(K), k(X)]),
    rev([k(LShift), Thing::FunctionKey, k(RGui), k(LAlt), k(LCtrl), Thing::LeftSymbolKey]),
        [k(F), k(G), k(C), k(R), k(L), k(Slash)],
        [k(D), k(H), k(T), k(N), k(S), k(Minus)],
        [k(B), k(M), k(W), k(V), k(Z), Thing::NavKey],
        [Thing::RightSymbolKey, k(Space), k(LGui), k(RCtrl), k(RAlt), k(RShift)],
];

/// Layer for typing numbers and symbols
pub const LAYER_SYMBOLS: Layer = [
    rev([k(Grave), shift(Kc8), k(Kc9), k(Kc8), k(Kc7), shift(RightBracket)]),
    rev([k(Backspace), k(Backslash), k(Kc6), k(Kc5), k(Kc4), shift(Kc5)]),
    rev([shift(Kc2), k(Kc0), k(Kc3), k(Kc2), k(Kc1), k(Quote)]),
    rev([k(LShift), Thing::FunctionKey, k(RGui), k(LAlt), k(LCtrl), Thing::LeftSymbolKey]),
        [shift(Kc4), k(Minus), k(Equal), shift(Kc6), shift(Kc7), shift(Kc1)],
        [k(RightBracket), shift(Kc9), shift(Kc0), shift(Kc3), k(LeftBracket), k(Enter)],
        [DFA, shift(Minus), shift(Equal), shift(Grave), shift(Backslash), Thing::NavKey],
        [Thing::RightSymbolKey, k(Space), k(LGui), k(RCtrl), k(RAlt), k(RShift)],
];

/// Same, but with a couple of changes for dvorak emulation
pub const LAYER_DVORAK_EMU_SYMBOLS: Layer = [
    rev([k(Grave), shift(Kc8), k(Kc9), k(Kc8), k(Kc7), shift(Equal)]),
    rev([k(Backspace), k(Backslash), k(Kc6), k(Kc5), k(Kc4), shift(Kc5)]),
    rev([shift(Kc2), k(Kc0), k(Kc3), k(Kc2), k(Kc1), k(Minus)]),
    rev([k(LShift), Thing::FunctionKey, k(RGui), k(LAlt), k(LCtrl), Thing::LeftSymbolKey]),
        [shift(Kc4), k(LeftBracket), k(RightBracket), shift(Kc6), shift(Kc7), shift(Kc1)],
        [k(Equal), shift(Kc9), shift(Kc0), shift(Kc3), k(Slash), k(Enter)],
        [DFA, shift(LeftBracket), shift(RightBracket), shift(Grave), shift(Backslash), Thing::NavKey],
        [Thing::RightSymbolKey, k(Space), k(LGui), k(RCtrl), k(RAlt), k(RShift)],
];

/// Layer for F-keys, arrows and other "navigation" keys
pub const LAYER_NAVIGATION: Layer = [
    rev([k(F15), k(F12), k(F9), k(F8), k(F7), DFA]),
    rev([k(F14), k(F11), k(F6), k(F5), k(F4), DFA]),
    rev([k(F13), k(F10), k(F3), k(F2), k(F1), DFA]),
    rev([k(LShift), Thing::FunctionKey, k(RGui), k(LAlt), k(LCtrl), Thing::LeftSymbolKey]),
        [k(Delete), k(U), k(I), k(O), k(P), DFA],
        [DFA, k(Left), k(Down), k(UP), k(Right), k(Enter)],
        [DFA, k(Home), k(PageDown), k(PageUp), k(End), Thing::NavKey],
        [Thing::RightSymbolKey, k(Space), k(LGui), k(RCtrl), k(RAlt), k(RShift)],
];

/// Layer for changing modes, and special keys like volume
pub const LAYER_FUNCTION: Layer = [
    rev([DFA, DFA, DFA, DFA, DFA, DFA]),
    rev([DFA, DFA, DFA, DFA, DFA, DFA]),
    rev([DFA, DFA, DFA, DFA, DFA, DFA]),
    rev([DFA, Thing::FunctionKey, DFA, DFA, DFA, Thing::LeftSymbolKey]),
        [DFA, DFA, DFA, DFA, DFA, DFA],
        [Thing::DvorakToggle, k(KbMute), k(KbVolumeDown), k(KbVolumeUp), Thing::StenoToggle, DFA],
        [DFA, DFA, DFA, DFA, DFA, Thing::NavKey],
        [Thing::RightSymbolKey, DFA, DFA, DFA, DFA, DFA],
];

/// Translate a [StenoKeyCode] into a valid [Thing]
macro_rules! st {
    ($i:ident) => { Thing::StenoKey(StenoKeyCode::$i.to_packet_code()) }
}

/// Layer for sending serial codes like a stenotype machine (Gemini PR protocol)
pub const LAYER_STENO: Layer = [
    rev([DFA, st!(S1), st!(TL), st!(PL), st!(HL), st!(ST1)]),
    rev([DFA, st!(S2), st!(KL), st!(WL), st!(RL), st!(ST2)]),
    rev([DFA, DFA, DFA, DFA, DFA, DFA]),
    rev([st!(Number), Thing::StenoToggle, DFA, st!(A), st!(O), Thing::LeftSymbolKey]),
        [st!(ST3), st!(FR), st!(PR), st!(LR), st!(TR), st!(DR)],
        [st!(ST4), st!(RR), st!(BR), st!(GR), st!(SR), st!(ZR)],
        [DFA, DFA, DFA, DFA, DFA, Thing::NavKey],
        [Thing::RightSymbolKey, st!(E), st!(U), DFA, DFA, st!(Number)],
];
