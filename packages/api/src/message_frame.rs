// TODO rewrite, written by AI bc of time pressure
use serde::{Deserialize, Serialize};

pub const HEADER_LEN: usize = 11;
pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;
pub const FLAG_MORE_FRAGMENTS: u8 = 0x01;
pub const NO_FRONTEND: u32 = 0;
pub const ALL_FRONTENDS: u32 = u32::MAX;

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct PluginMessageFrame {
    pub frontend_process_id: u32,
    pub message_id: u32,
    pub flags: u8,
    pub logical_channel: String,
    pub payload: Vec<u8>,
}

fn invalid(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_string())
}

impl PluginMessageFrame {
    /// Body only, without the u32 length prefix.
    pub fn encode(&self) -> std::io::Result<Vec<u8>> {
        let channel = self.logical_channel.as_bytes();
        if channel.len() > u16::MAX as usize { return Err(invalid("channel too long")); }
        if self.flags & !FLAG_MORE_FRAGMENTS != 0 { return Err(invalid("unknown flags")); }
        let len = HEADER_LEN + channel.len() + self.payload.len();
        if len > MAX_FRAME_SIZE { return Err(invalid("frame too large")); }

        let mut b = Vec::with_capacity(len);
        b.extend_from_slice(&self.frontend_process_id.to_le_bytes());
        b.extend_from_slice(&self.message_id.to_le_bytes());
        b.push(self.flags);
        b.extend_from_slice(&(channel.len() as u16).to_le_bytes());
        b.extend_from_slice(channel);
        b.extend_from_slice(&self.payload);
        Ok(b)
    }

    pub fn decode(body: &[u8]) -> std::io::Result<Self> {
        if body.len() < HEADER_LEN { return Err(invalid("frame shorter than header")); }
        let flags = body[8];
        if flags & !FLAG_MORE_FRAGMENTS != 0 { return Err(invalid("unknown flags")); }
        let channel_len = u16::from_le_bytes([body[9], body[10]]) as usize;
        let channel_end = HEADER_LEN + channel_len;
        if channel_end > body.len() { return Err(invalid("channel past end of frame")); }
        let logical_channel = std::str::from_utf8(&body[HEADER_LEN..channel_end])
            .map_err(|_| invalid("channel is not UTF-8"))?
            .to_string();
        Ok(Self {
            frontend_process_id: u32::from_le_bytes(body[0..4].try_into().unwrap()),
            message_id: u32::from_le_bytes(body[4..8].try_into().unwrap()),
            flags,
            logical_channel,
            payload: body[channel_end..].to_vec(),
        })
    }
}