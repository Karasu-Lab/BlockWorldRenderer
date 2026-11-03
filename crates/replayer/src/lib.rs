//! Replay playback behind an interface.
//!
//! A recording is a timed stream of protocol packets. This crate owns
//! the stream and the playback cursor; what a packet *does* belongs to
//! the [`PacketSink`] the caller supplies, so the format support (the
//! `.mcpr` container the ReplayMod writes, behind the `mcpr` feature)
//! stays decoupled from the world the packets are played into.

use std::fmt;

/// What a recording says about itself, as stored in its metadata.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// Total length of the recording, in milliseconds.
    pub duration_ms: u32,
    /// When the recording was made, in epoch milliseconds.
    pub date_ms: i64,
    /// The game version the recording was made with.
    pub mc_version: String,
    /// The protocol version, when the metadata carries one.
    pub protocol: Option<i32>,
    /// The recording file format version.
    pub file_format_version: i32,
    pub singleplayer: bool,
    pub server_name: String,
    /// Entity id of the player who made the recording, or -1.
    pub self_id: i32,
    /// The players present, as UUID strings.
    pub players: Vec<String>,
}

/// One recorded packet, at its offset into the recording.
///
/// `data` is the packet exactly as it went over the wire: the VarInt
/// packet id followed by the packet body, with no length prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub time_ms: u32,
    pub data: Vec<u8>,
}

/// Why a recording could not be opened.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The crate was built without a recording format implementation.
    NoBackend,
    /// The input is not a recording this crate can read.
    Parse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoBackend => write!(f, "no replay format is enabled"),
            Error::Parse(message) => write!(f, "invalid recording: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// A recording that can be played.
pub trait Replay {
    fn metadata(&self) -> &Metadata;
    /// Every packet in the recording, in the order recorded.
    fn packets(&self) -> &[Packet];
}

/// The world side of playback: where packets are applied.
pub trait PacketSink {
    /// Applies one packet, in recording order.
    fn apply(&mut self, packet: &[u8]);
    /// Returns the world to its pre-recording state, so playback can
    /// restart from the beginning (a backward seek).
    fn reset(&mut self);
}

/// Opens a recording from its bytes, using the format the build
/// selected.
pub fn open(bytes: &[u8]) -> Result<Box<dyn Replay>, Error> {
    #[cfg(feature = "mcpr")]
    {
        Ok(Box::new(mcpr::McprReplay::open(bytes)?))
    }
    #[cfg(not(feature = "mcpr"))]
    {
        let _ = bytes;
        Err(Error::NoBackend)
    }
}

/// A playback position inside one recording.
///
/// The caller owns the clock (and with it pause and speed): it tells
/// the cursor where the playhead is, and the cursor applies the
/// packets between the old and the new position to the sink. Moving
/// forward applies the packets in between; moving backward resets the
/// sink and replays from the start, so the sink never has to undo.
pub struct Playback<'a> {
    packets: &'a [Packet],
    position_ms: u32,
    /// Index of the first packet not yet applied.
    next: usize,
}

impl<'a> Playback<'a> {
    pub fn new(replay: &'a dyn Replay) -> Self {
        Self {
            packets: replay.packets(),
            position_ms: 0,
            next: 0,
        }
    }

    pub fn position_ms(&self) -> u32 {
        self.position_ms
    }

    /// Moves the playhead to `target_ms`, applying packets to `sink`.
    pub fn advance_to(&mut self, target_ms: u32, sink: &mut dyn PacketSink) {
        if target_ms < self.position_ms {
            sink.reset();
            self.next = 0;
        }
        while let Some(packet) = self.packets.get(self.next) {
            if packet.time_ms > target_ms {
                break;
            }
            sink.apply(&packet.data);
            self.next += 1;
        }
        self.position_ms = target_ms;
    }

    /// Whether every packet has been applied.
    pub fn is_finished(&self) -> bool {
        self.next >= self.packets.len()
    }
}

#[cfg(feature = "mcpr")]
mod mcpr {
    use std::io::{Cursor, Read};

    use serde::Deserialize;

    use super::{Error, Metadata, Packet, Replay};

    const ENTRY_RECORDING: &str = "recording.tmcpr";
    const ENTRY_META_DATA: &str = "metaData.json";

    /// A `.mcpr` file: a zip whose `recording.tmcpr` entry is the
    /// packet stream and whose `metaData.json` describes it.
    ///
    /// The stream is a sequence of records until the end of the entry:
    /// a big-endian i32 timestamp in milliseconds, a big-endian i32
    /// byte length, and that many bytes of packet (VarInt id + body).
    pub struct McprReplay {
        metadata: Metadata,
        packets: Vec<Packet>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct MetaData {
        #[serde(default)]
        singleplayer: bool,
        #[serde(default)]
        server_name: String,
        #[serde(default)]
        duration: u32,
        #[serde(default)]
        date: i64,
        #[serde(default)]
        mcversion: String,
        #[serde(default)]
        file_format_version: i32,
        #[serde(default)]
        protocol: Option<i32>,
        #[serde(default)]
        self_id: i32,
        #[serde(default)]
        players: Vec<String>,
    }

    impl McprReplay {
        pub fn open(bytes: &[u8]) -> Result<Self, Error> {
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
                .map_err(|error| Error::Parse(format!("not a zip archive: {error}")))?;

            let meta_data = read_entry(&mut archive, ENTRY_META_DATA)?;
            let meta: MetaData = serde_json::from_slice(&meta_data)
                .map_err(|error| Error::Parse(format!("bad {ENTRY_META_DATA}: {error}")))?;

            let recording = read_entry(&mut archive, ENTRY_RECORDING)?;
            let packets = parse_stream(&recording)?;

            Ok(Self {
                metadata: Metadata {
                    duration_ms: meta.duration,
                    date_ms: meta.date,
                    mc_version: meta.mcversion,
                    protocol: meta.protocol,
                    file_format_version: meta.file_format_version,
                    singleplayer: meta.singleplayer,
                    server_name: meta.server_name,
                    self_id: meta.self_id,
                    players: meta.players,
                },
                packets,
            })
        }
    }

    impl Replay for McprReplay {
        fn metadata(&self) -> &Metadata {
            &self.metadata
        }

        fn packets(&self) -> &[Packet] {
            &self.packets
        }
    }

    fn read_entry(
        archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
        name: &str,
    ) -> Result<Vec<u8>, Error> {
        let mut entry = archive
            .by_name(name)
            .map_err(|_| Error::Parse(format!("the archive has no {name} entry")))?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| Error::Parse(format!("cannot read {name}: {error}")))?;
        Ok(bytes)
    }

    fn parse_stream(bytes: &[u8]) -> Result<Vec<Packet>, Error> {
        let mut packets = Vec::new();
        let mut rest = bytes;
        while !rest.is_empty() {
            if rest.len() < 8 {
                return Err(Error::Parse("the recording ends mid-record".into()));
            }
            let time_ms = u32::from_be_bytes(rest[0..4].try_into().unwrap());
            let length = u32::from_be_bytes(rest[4..8].try_into().unwrap()) as usize;
            rest = &rest[8..];
            if rest.len() < length {
                return Err(Error::Parse("the recording ends mid-packet".into()));
            }
            packets.push(Packet {
                time_ms,
                data: rest[..length].to_vec(),
            });
            rest = &rest[length..];
        }
        Ok(packets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RecordingSink {
        applied: Vec<Vec<u8>>,
        resets: usize,
    }

    impl PacketSink for RecordingSink {
        fn apply(&mut self, packet: &[u8]) {
            self.applied.push(packet.to_vec());
        }

        fn reset(&mut self) {
            self.resets += 1;
            self.applied.clear();
        }
    }

    struct StubReplay {
        metadata: Metadata,
        packets: Vec<Packet>,
    }

    impl Replay for StubReplay {
        fn metadata(&self) -> &Metadata {
            &self.metadata
        }

        fn packets(&self) -> &[Packet] {
            &self.packets
        }
    }

    fn stub() -> StubReplay {
        StubReplay {
            metadata: Metadata::default(),
            packets: vec![
                Packet {
                    time_ms: 0,
                    data: vec![1],
                },
                Packet {
                    time_ms: 100,
                    data: vec![2],
                },
                Packet {
                    time_ms: 300,
                    data: vec![3],
                },
            ],
        }
    }

    #[test]
    fn playback_applies_packets_up_to_the_playhead() {
        let replay = stub();
        let mut playback = Playback::new(&replay);
        let mut sink = RecordingSink {
            applied: Vec::new(),
            resets: 0,
        };

        playback.advance_to(150, &mut sink);
        assert_eq!(sink.applied, vec![vec![1], vec![2]]);
        assert_eq!(playback.position_ms(), 150);
        assert!(!playback.is_finished());

        playback.advance_to(300, &mut sink);
        assert_eq!(sink.applied, vec![vec![1], vec![2], vec![3]]);
        assert!(playback.is_finished());
    }

    #[test]
    fn a_backward_seek_resets_and_replays() {
        let replay = stub();
        let mut playback = Playback::new(&replay);
        let mut sink = RecordingSink {
            applied: Vec::new(),
            resets: 0,
        };

        playback.advance_to(300, &mut sink);
        playback.advance_to(50, &mut sink);
        assert_eq!(sink.resets, 1);
        assert_eq!(sink.applied, vec![vec![1]]);
        assert_eq!(playback.position_ms(), 50);
    }

    #[cfg(feature = "mcpr")]
    #[test]
    fn opens_an_mcpr_archive() {
        use std::io::{Cursor, Write};

        let mut stream = Vec::new();
        for (time_ms, packet) in [(0u32, vec![0x2bu8, 1, 2]), (50, vec![0x10, 3])] {
            stream.extend(time_ms.to_be_bytes());
            stream.extend((packet.len() as u32).to_be_bytes());
            stream.extend(packet);
        }

        let mut zip_bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut zip_bytes));
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("metaData.json", options).unwrap();
            writer
                .write_all(
                    br#"{"singleplayer":true,"duration":1200,"date":1700000000000,"mcversion":"1.21.4","fileFormat":"MCPR","fileFormatVersion":14,"protocol":769,"selfId":7,"players":[]}"#,
                )
                .unwrap();
            writer.start_file("recording.tmcpr", options).unwrap();
            writer.write_all(&stream).unwrap();
            writer.finish().unwrap();
        }

        let replay = open(&zip_bytes).expect("the archive opens");
        let metadata = replay.metadata();
        assert_eq!(metadata.duration_ms, 1200);
        assert_eq!(metadata.mc_version, "1.21.4");
        assert_eq!(metadata.protocol, Some(769));
        assert_eq!(metadata.self_id, 7);
        assert!(metadata.singleplayer);

        let packets = replay.packets();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].time_ms, 0);
        assert_eq!(packets[0].data, vec![0x2b, 1, 2]);
        assert_eq!(packets[1].time_ms, 50);
    }

    #[cfg(feature = "mcpr")]
    #[test]
    fn rejects_a_non_archive() {
        assert!(matches!(open(b"not a replay"), Err(Error::Parse(_))));
    }

    #[cfg(not(feature = "mcpr"))]
    #[test]
    fn reports_the_missing_backend() {
        assert_eq!(open(b"anything").err(), Some(Error::NoBackend));
    }
}
