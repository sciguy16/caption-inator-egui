use ogg::{OggPacket, OggPacketType};
use std::{
    collections::VecDeque,
    fmt::Debug,
    time::{Duration, Instant},
};

const EXPIRY: Duration = Duration::from_secs(5);

pub struct SampleBuffer {
    stream_id: u32,
    front_packet_index: u32,
    front_granule_position: u64,
    buf: VecDeque<Sample>,
}

impl SampleBuffer {
    pub const fn new() -> Self {
        Self {
            stream_id: 0,
            front_packet_index: 2,
            front_granule_position: 0,
            buf: VecDeque::new(),
        }
    }

    pub const fn stream_id(&self) -> u32 {
        self.stream_id
    }

    pub const fn front_packet_index(&self) -> u32 {
        self.front_packet_index
    }

    pub const fn front_granule_position(&self) -> u64 {
        self.front_granule_position
    }

    pub fn add_packet(&mut self, packet: OggPacket) {
        self.clear();
        self.stream_id = packet.stream_id;
        self.buf.push_back(packet.into());
    }

    fn clear(&mut self) {
        // Keep at least one entry
        while self.buf.len() > 1
            && self.buf.pop_front_if(|front| front.has_expired()).is_some()
        {
        }

        if let Some(front) = self.buf.front() {
            self.front_packet_index = front.packet.packet_index;
            self.front_granule_position = front.packet.granule_position;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = OggPacket> + use<'_> {
        let mut is_first = true;
        self.buf.iter().map(move |sample| {
            let mut pkt = sample.packet.clone();
            if is_first {
                pkt.packet_type = OggPacketType::BeginOfStream;
                is_first = false;
            }
            pkt
        })
    }
}

struct Sample {
    ts: Instant,
    packet: OggPacket,
}

impl From<OggPacket> for Sample {
    fn from(packet: OggPacket) -> Self {
        Self {
            ts: Instant::now(),
            packet,
        }
    }
}

impl Debug for Sample {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("Sample")
            .field("elapsed", &self.ts.elapsed())
            .field("packet", &format_args!("{:02x?}", self.packet))
            .finish()
    }
}

impl Sample {
    fn has_expired(&self) -> bool {
        self.ts.elapsed() >= EXPIRY
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn sleep() {
        std::thread::sleep(Duration::from_secs(1));
    }

    #[test]
    fn test_buffer() {
        let mut buf = SampleBuffer::new();
        let pkt = OggPacket::default();
        assert!(buf.buf.is_empty());
        buf.add_packet(pkt.clone());
        sleep();
        buf.add_packet(pkt.clone());
        sleep();
        buf.add_packet(pkt.clone());
        sleep();
        buf.add_packet(pkt.clone());
        sleep();
        assert_eq!(buf.buf.len(), 4);
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(buf.buf.len(), 4);
        sleep();
        buf.clear();
        assert_eq!(buf.buf.len(), 3);
        sleep();
        buf.add_packet(pkt.clone());
        assert_eq!(buf.buf.len(), 3);
        sleep();
        buf.clear();
        assert_eq!(buf.buf.len(), 2);
        sleep();
        buf.clear();
        assert_eq!(buf.buf.len(), 1);
        sleep();
        sleep();
        sleep();
        buf.clear();
        dbg!(&buf.buf);
        assert!(buf.buf.is_empty());
    }
}
