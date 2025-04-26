//! Defines keycodes for stenotype input, linked to [PacketCode]s corresponding to flag bits
//! according to the [Gemini PR protocol](https://github.com/openstenoproject/plover/blob/main/plover/machine/geminipr.py).

type BytePosition = u8;
type Flag = u8;
pub type PacketCode = (BytePosition, Flag);
pub type Packet = [u8; 6];

#[derive(Clone, Copy, Debug)]
pub enum KeyCode {
    ST1, ST2, ST3, ST4,
    S1, TL, PL, HL,
    S2, KL, WL, RL,
    A, O, E, U,
    FR, PR, LR, TR, DR,
    RR, BR, GR, SR, ZR,
    Number,
}

impl KeyCode {
    pub const fn to_packet_code(self) -> PacketCode {
        match self {
            KeyCode::S1 => (1,64),
            KeyCode::TL => (1,16),
            KeyCode::PL => (1,4),
            KeyCode::HL => (1,1),

            KeyCode::S2 => (1,32),
            KeyCode::KL => (1,8),
            KeyCode::WL => (1,2),
            KeyCode::RL => (2,64),

            KeyCode::ST1 => (2,8),
            KeyCode::ST2 => (2,4),
            KeyCode::ST3 => (3,32),
            KeyCode::ST4 => (3,16),

            KeyCode::A => (2,32),
            KeyCode::O => (2,16),
            KeyCode::E => (3,8),
            KeyCode::U => (3,4),

            KeyCode::FR => (3,2),
            KeyCode::PR => (4,64),
            KeyCode::LR => (4,16),
            KeyCode::TR => (4,4),
            KeyCode::DR => (4,1),

            KeyCode::RR => (3,1),
            KeyCode::BR => (4,32),
            KeyCode::GR => (4,8),
            KeyCode::SR => (4,2),
            KeyCode::ZR => (5,1),

            KeyCode::Number => (0, 32),  // #1 according to the GeminiPR keymap
        }
    }
}
