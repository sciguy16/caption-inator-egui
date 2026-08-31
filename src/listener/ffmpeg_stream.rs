use super::sample_buffer::SampleBuffer;
use crate::Result;
use ogg::OggPacket;
use ogg::OggPacketType;
use std::process::Stdio;
use tokio::{
    io::{AsyncRead, AsyncReadExt, BufReader},
    sync::mpsc,
};
use tokio_stream::{Stream, wrappers::ReceiverStream};

pub struct FfmpegBuffer {
    subscribe_tx: mpsc::Sender<mpsc::Sender<Vec<u8>>>,
    child_handle: tokio::task::JoinHandle<()>,
    inner_handle: tokio::task::JoinHandle<()>,
}

impl FfmpegBuffer {
    // ffmpeg -y -f pulse -ac 1 -i default -f webm /dev/stdout
    pub async fn listen_from_default_input(format: &str) -> Result<Self> {
        let (stream_tx, _stream_rx) = mpsc::channel(10);
        let (subscribe_tx, subscribe_rx) = mpsc::channel(10);

        let mut child = tokio::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "pulse",
                "-ar",
                "16k",
                "-ac",
                "1",
                "-i",
                "default",
                "-f",
                format,
                "/dev/stdout",
            ])
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().unwrap();

        let child_handle = tokio::task::spawn(async move {
            child.wait().await.unwrap();
        });

        let inner = Inner::new(stdout, stream_tx, subscribe_rx);
        let inner_handle = inner.run();

        Ok(Self {
            subscribe_tx,
            child_handle,
            inner_handle,
        })
    }

    pub async fn subscribe(
        &self,
    ) -> Result<impl Stream<Item = Vec<u8>> + 'static> {
        let (stream_tx, stream_rx) = mpsc::channel(10);
        self.subscribe_tx.send(stream_tx).await?;

        Ok(ReceiverStream::new(stream_rx))
    }
}

impl Drop for FfmpegBuffer {
    fn drop(&mut self) {
        self.child_handle.abort();
        self.inner_handle.abort();
    }
}

struct Inner<R: AsyncRead> {
    reader: BufReader<R>,
    buf: Vec<u8>,
    ogg_decoder: OggDecoder,
    sample_buffer: SampleBuffer,
    stream_tx: mpsc::Sender<Vec<u8>>,
    subscribe_rx: mpsc::Receiver<mpsc::Sender<Vec<u8>>>,
    packet_index_offset: u32,
    granule_position_offset: u64,
}

impl<R> Inner<R>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    fn new(
        reader: R,
        stream_tx: mpsc::Sender<Vec<u8>>,
        subscribe_rx: mpsc::Receiver<mpsc::Sender<Vec<u8>>>,
    ) -> Self {
        Self {
            reader: BufReader::new(reader),
            buf: vec![0; 2048],
            ogg_decoder: OggDecoder::new(),
            sample_buffer: SampleBuffer::new(),
            stream_tx,
            subscribe_rx,
            packet_index_offset: 0,
            granule_position_offset: 0,
        }
    }

    fn run(self) -> tokio::task::JoinHandle<()> {
        tokio::task::spawn(self.run_inner())
    }

    async fn run_inner(mut self) {
        let mut errors = 0;
        loop {
            let new_stream_tx = if self.stream_tx.is_closed() {
                trace!("Wait for subscriber");
                self.subscribe_rx.recv().await
            } else {
                self.subscribe_rx.try_recv().ok()
            };

            if let Some(new_stream_tx) = new_stream_tx {
                trace!("Forward buffered data to new subscriber");
                self.stream_tx = new_stream_tx;
                self.packet_index_offset =
                    self.sample_buffer.front_packet_index() - 2;
                self.granule_position_offset =
                    self.sample_buffer.front_granule_position();

                self.send_opus_headers(self.sample_buffer.stream_id()).await;

                let packets = self.sample_buffer.iter().collect::<Vec<_>>();
                self.send_packets(packets).await;
            }

            match self.reader.read_exact(&mut self.buf).await {
                Ok(_) => errors = 0,
                Err(err) => {
                    warn!("{err}");
                    errors += 1;
                    if errors > 5 {
                        error!(
                            "Max errors reached, unable to read ffmpeg stream"
                        );
                        break;
                    }
                    continue;
                }
            }

            if self.stream_tx.capacity() < 2 {
                warn!("channel nearly full!");
            }

            let packets = self.ogg_decoder.add_sample(&self.buf);
            for packet in &packets {
                self.sample_buffer.add_packet(packet.clone());
            }
            self.send_packets(packets).await;
        }
    }

    async fn send_packets(
        &mut self,
        packets: impl IntoIterator<Item = OggPacket>,
    ) {
        for mut packet in packets {
            packet.packet_index -= self.packet_index_offset;
            packet.granule_position -= self.granule_position_offset;
            self.send_packet(packet).await;
        }
    }

    async fn send_packet(&mut self, packet: OggPacket) {
        if self.stream_tx.send(packet.into_bytes()).await.is_err() {
            info!("Stream closed");
        }
    }

    async fn send_opus_headers(&mut self, stream_id: u32) {
        for packet in make_opus_headers(stream_id) {
            self.send_packet(packet).await;
        }
    }
}

pub struct OggDecoder {
    read_buf: Vec<u8>,
}

impl OggDecoder {
    pub const fn new() -> Self {
        Self {
            read_buf: Vec::new(),
        }
    }

    pub fn add_sample(&mut self, sample: &[u8]) -> Vec<OggPacket> {
        self.read_buf.extend(sample);
        self.decode_packets()
    }

    fn decode_packets(&mut self) -> Vec<OggPacket> {
        let mut decoded = Vec::new();
        loop {
            let mut packet_length = 0;
            match OggPacket::from_bytes(&self.read_buf, &mut packet_length) {
                Ok(packet) => {
                    // dbg!(&packet);
                    dbg!((packet.packet_type, packet.packet_index));
                    decoded.push(packet);
                    self.read_buf = self.read_buf[packet_length..].to_vec();
                }
                Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Not enoufh data; try again later
                    break;
                }
                Err(err) => {
                    // Genuine decoding error
                    error!("{err}");
                }
            }
        }
        decoded
    }
}

fn make_opus_headers(stream_id: u32) -> impl Iterator<Item = OggPacket> {
    const HEADER: &[u8] =
        &hex_literal::hex!("4f707573486561640102380180bb0000000000");
    const TAGS: &[u8] = &hex_literal::hex!(
        "4f707573546167730d0000004c61766636322e31322e313032010000001d0000
             00656e636f6465723d4c61766336322e32382e313032206c69626f707573"
    );

    [
        OggPacket {
            packet_type: OggPacketType::BeginOfStream,
            packet_index: 0,
            stream_id,
            segment_table: vec![HEADER.len().try_into().unwrap()],
            data: HEADER.to_vec(),
            ..OggPacket::default()
        },
        OggPacket {
            packet_type: OggPacketType::Continuation,
            packet_index: 1,
            stream_id,
            segment_table: vec![TAGS.len().try_into().unwrap()],
            data: TAGS.to_vec(),
            ..OggPacket::default()
        },
    ]
    .into_iter()
}

#[cfg(test)]
mod test {
    use super::*;
    use ogg::OggPacketType;

    const OPUS_STREAM: &[u8] =
        include_bytes!("../../test-data/opus-stream.ogg");

    #[derive(Debug, PartialEq)]
    pub struct OggPacketInfo {
        packet_type: OggPacketType,
        granule_position: u64,
        stream_id: u32,
        packet_index: u32,
        checksum: u32,
        segment_table_len: usize,
        data_len: usize,
    }

    impl From<&OggPacket> for OggPacketInfo {
        fn from(packet: &OggPacket) -> Self {
            Self {
                packet_type: packet.packet_type,
                granule_position: packet.granule_position,
                stream_id: packet.stream_id,
                packet_index: packet.packet_index,
                checksum: packet.checksum,
                segment_table_len: packet.segment_table.len(),
                data_len: packet.data.len(),
            }
        }
    }

    #[test]
    fn test_file_stream() {
        let mut decoder = OggDecoder::new();
        let packets = decoder.add_sample(OPUS_STREAM);

        assert_eq!(packets.len(), 4);
        let info = packets.iter().map(OggPacketInfo::from).collect::<Vec<_>>();

        assert_eq!(
            info,
            [
                OggPacketInfo {
                    packet_type: OggPacketType::BeginOfStream,
                    granule_position: 0,
                    stream_id: 3943044422,
                    packet_index: 0,
                    checksum: 1462585564,
                    segment_table_len: 1,
                    data_len: 19,
                },
                OggPacketInfo {
                    packet_type: OggPacketType::Continuation,
                    granule_position: 0,
                    stream_id: 3943044422,
                    packet_index: 1,
                    checksum: 3871998890,
                    segment_table_len: 1,
                    data_len: 62,
                },
                OggPacketInfo {
                    packet_type: OggPacketType::Continuation,
                    granule_position: 47782,
                    stream_id: 3943044422,
                    packet_index: 2,
                    checksum: 4148091861,
                    segment_table_len: 61,
                    data_len: 12584,
                },
                OggPacketInfo {
                    packet_type: OggPacketType::EndOfStream,
                    granule_position: 93332,
                    stream_id: 3943044422,
                    packet_index: 3,
                    checksum: 3190996898,
                    segment_table_len: 52,
                    data_len: 11009,
                },
            ],
        );

        /*
        00000000: 4f70 7573 4865 6164 0102 3801 80bb 0000  OpusHead..8.....
        00000010: 0000 00                                  ...
        */
        assert_eq!(
            hex::encode(&packets[0].data),
            "4f707573486561640102380180bb0000000000",
        );

        /*
        00000000: 4f70 7573 5461 6773 0d00 0000 4c61 7666  OpusTags....Lavf
        00000010: 3632 2e31 322e 3130 3201 0000 001d 0000  62.12.102.......
        00000020: 0065 6e63 6f64 6572 3d4c 6176 6336 322e  .encoder=Lavc62.
        00000030: 3238 2e31 3032 206c 6962 6f70 7573       28.102 libopus
        */
        assert_eq!(
            hex::encode(&packets[1].data),
            "4f707573546167730d0000004c61766636322e31322e313032010000001d0000\
             00656e636f6465723d4c61766336322e32382e313032206c69626f707573",
        );

        let encoded = packets
            .iter()
            .flat_map(|packet| packet.clone().into_bytes())
            .collect::<Vec<_>>();
        assert_eq!(encoded, OPUS_STREAM);
    }
}
