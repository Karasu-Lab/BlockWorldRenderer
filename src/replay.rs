//! A replay being played into the viewer.
//!
//! The recording's packets are applied to a [`ReplayState`] — the chunk
//! bodies it sent and the entities it knows — which a [`WorldSource`]
//! then serves to the viewer. The session owns the playhead: moving it
//! forward applies the packets in between, moving it backward clears
//! the state and replays from the start.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::entity::EntityInfo;
use pomme_client::viewer::steel::SteelWorld;
use pomme_client::viewer::{BiomeClimate, Dimension, Environment, WorldSource};
use pomme_protocol::packets::{Direction, PacketTable, Phase};
use replayer::{Metadata, Replay};

/// The world a recording describes so far.
#[derive(Default)]
pub struct ReplayState {
    chunks: HashMap<(i32, i32), Vec<u8>>,
    entities: HashMap<i32, EntityInfo>,
    /// Player UUIDs from the player-info packet, with their names.
    players: HashMap<String, Option<String>>,
    self_id: i32,
    chunk_id: Option<u32>,
    forget_chunk_id: Option<u32>,
    add_entity_id: Option<u32>,
    teleport_ids: HashMap<u32, TeleportKind>,
    move_pos_id: Option<u32>,
    move_pos_rot_id: Option<u32>,
    move_rot_id: Option<u32>,
    remove_entities_id: Option<u32>,
    player_info_id: Option<u32>,
}

#[derive(Clone, Copy)]
enum TeleportKind {
    /// Pre-1.21.2 layout: position, then byte angles.
    Legacy,
    /// 1.21.2+ layout: position, velocity, then float angles.
    Synced,
}

impl ReplayState {
    fn new(metadata: &Metadata) -> Self {
        let mut state = Self {
            self_id: metadata.self_id,
            ..Self::default()
        };
        let table = metadata
            .protocol
            .and_then(PacketTable::for_protocol)
            .unwrap_or_else(PacketTable::native);
        let id = |name: &str| table.id(Phase::Game, Direction::Clientbound, name);
        state.chunk_id = id("level_chunk_with_light");
        state.forget_chunk_id = id("forget_level_chunk");
        state.add_entity_id = id("add_entity");
        state.move_pos_id = id("move_entity_pos");
        state.move_pos_rot_id = id("move_entity_pos_rot");
        state.move_rot_id = id("move_entity_rot");
        state.remove_entities_id = id("remove_entities");
        state.player_info_id = id("player_info_update");
        for (name, kind) in [
            ("teleport_entity", TeleportKind::Legacy),
            ("entity_position_sync", TeleportKind::Synced),
            ("sync_entity_position", TeleportKind::Synced),
        ] {
            if let Some(found) = id(name) {
                state.teleport_ids.insert(found, kind);
            }
        }
        state
    }

    fn reset(&mut self) {
        self.chunks.clear();
        self.entities.clear();
        self.players.clear();
    }

    /// The entities currently known, players included.
    pub fn entities(&self) -> Vec<EntityInfo> {
        let mut entities: Vec<EntityInfo> = self.entities.values().cloned().collect();
        for (uuid, name) in &self.players {
            if !entities.iter().any(|e| e.uuid.as_deref() == Some(uuid)) {
                entities.push(EntityInfo {
                    id: None,
                    uuid: Some(uuid.clone()),
                    name: name.clone(),
                    is_player: true,
                    position: None,
                    yaw_deg: 0.0,
                    pitch_deg: 0.0,
                });
            }
        }
        entities.sort_by_key(|e| e.id.unwrap_or(i32::MAX));
        entities
    }

    fn entity(&mut self, id: i32) -> &mut EntityInfo {
        self.entities.entry(id).or_insert_with(|| EntityInfo {
            id: Some(id),
            uuid: None,
            name: None,
            is_player: false,
            position: None,
            yaw_deg: 0.0,
            pitch_deg: 0.0,
        })
    }

    fn apply(&mut self, packet: &[u8]) {
        let mut body = Body::new(packet);
        let Some(id) = body.varint() else { return };
        let Some(id) = u32::try_from(id).ok() else {
            return;
        };
        if Some(id) == self.chunk_id {
            // The viewer decodes chunk bodies from the position on, so
            // keep the whole body, not what follows it.
            let whole_body = body.rest().to_vec();
            if let (Some(x), Some(z)) = (body.i32(), body.i32()) {
                self.chunks.insert((x, z), whole_body);
            }
        } else if Some(id) == self.forget_chunk_id {
            if let (Some(x), Some(z)) = (body.i32(), body.i32()) {
                self.chunks.remove(&(x, z));
            }
        } else if Some(id) == self.add_entity_id {
            self.apply_add_entity(&mut body);
        } else if let Some(kind) = self.teleport_ids.get(&id).copied() {
            self.apply_teleport(&mut body, kind);
        } else if Some(id) == self.move_pos_id || Some(id) == self.move_pos_rot_id {
            let rotated = Some(id) == self.move_pos_rot_id;
            if let Some(entity_id) = body.varint() {
                let delta = [body.i16(), body.i16(), body.i16()];
                let (yaw, pitch) = if rotated {
                    (body.i8(), body.i8())
                } else {
                    (None, None)
                };
                let entity = self.entity(entity_id);
                if let Some(position) = &mut entity.position {
                    for (axis, delta) in delta.iter().enumerate() {
                        if let Some(delta) = delta {
                            position[axis] += f64::from(*delta) / 4096.0;
                        }
                    }
                }
                if let Some(yaw) = yaw {
                    entity.yaw_deg = angle_deg(yaw);
                }
                if let Some(pitch) = pitch {
                    entity.pitch_deg = angle_deg(pitch);
                }
            }
        } else if Some(id) == self.move_rot_id {
            if let (Some(entity_id), yaw, pitch) = (body.varint(), body.i8(), body.i8()) {
                let entity = self.entity(entity_id);
                if let Some(yaw) = yaw {
                    entity.yaw_deg = angle_deg(yaw);
                }
                if let Some(pitch) = pitch {
                    entity.pitch_deg = angle_deg(pitch);
                }
            }
        } else if Some(id) == self.remove_entities_id {
            if let Some(count) = body.varint() {
                for _ in 0..count.max(0) {
                    if let Some(entity_id) = body.varint() {
                        self.entities.remove(&entity_id);
                    }
                }
            }
        } else if Some(id) == self.player_info_id {
            self.apply_player_info(&mut body);
        }
    }

    fn apply_add_entity(&mut self, body: &mut Body<'_>) {
        let Some(id) = body.varint() else { return };
        let uuid = body.uuid();
        let _type = body.varint();
        let position = [body.f64(), body.f64(), body.f64()];
        let pitch = body.i8();
        let yaw = body.i8();
        let is_self = id == self.self_id;
        let known_name = uuid
            .as_ref()
            .and_then(|uuid| self.players.get(uuid).cloned());
        let is_player = is_self || known_name.is_some();
        let entity = self.entity(id);
        entity.uuid = uuid;
        if let (Some(x), Some(y), Some(z)) = (position[0], position[1], position[2]) {
            entity.position = Some([x, y, z]);
        }
        if let Some(yaw) = yaw {
            entity.yaw_deg = angle_deg(yaw);
        }
        if let Some(pitch) = pitch {
            entity.pitch_deg = angle_deg(pitch);
        }
        if is_player {
            entity.is_player = true;
        }
        if let Some(name) = known_name {
            entity.name = name;
        }
    }

    fn apply_teleport(&mut self, body: &mut Body<'_>, kind: TeleportKind) {
        let Some(id) = body.varint() else { return };
        let position = [body.f64(), body.f64(), body.f64()];
        let (yaw, pitch) = match kind {
            TeleportKind::Synced => {
                let _velocity = [body.f64(), body.f64(), body.f64()];
                (body.f32(), body.f32())
            }
            TeleportKind::Legacy => (body.i8().map(angle_deg), body.i8().map(angle_deg)),
        };
        let entity = self.entity(id);
        if let (Some(x), Some(y), Some(z)) = (position[0], position[1], position[2]) {
            entity.position = Some([x, y, z]);
        }
        if let Some(yaw) = yaw {
            entity.yaw_deg = yaw;
        }
        if let Some(pitch) = pitch {
            entity.pitch_deg = pitch;
        }
    }

    fn apply_player_info(&mut self, body: &mut Body<'_>) {
        let Some(actions) = body.u8() else { return };
        let Some(count) = body.varint() else { return };
        for _ in 0..count.max(0) {
            let Some(uuid) = body.uuid() else { return };
            let mut name = None;
            if actions & 0x01 != 0 {
                name = body.string();
                // Properties: a count, then name/value/optional signature each.
                if let Some(properties) = body.varint() {
                    for _ in 0..properties.max(0) {
                        let _ = body.string();
                        let _ = body.string();
                        if body.u8() == Some(1) {
                            let _ = body.string();
                        }
                    }
                }
            }
            if actions & 0x08 != 0 {
                // Game mode (VarInt) precedes the listed flag in this action set.
                let _ = body.varint();
            }
            if actions & 0x10 != 0 {
                let _listed = body.u8();
            }
            self.players.insert(uuid.clone(), name.clone());
            let existing = self
                .entities
                .values()
                .find(|e| e.uuid.as_deref() == Some(&uuid))
                .and_then(|e| e.id);
            if let Some(entity_id) = existing {
                let entity = self.entity(entity_id);
                entity.is_player = true;
                if name.is_some() {
                    entity.name = name;
                }
            }
        }
    }
}

/// Byte angles in packets are 256 steps per turn.
fn angle_deg(byte: i8) -> f32 {
    f32::from(byte) * (360.0 / 256.0)
}

/// A cursor over a packet body; every read is optional so a malformed
/// or differently-versioned packet just stops applying.
struct Body<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Body<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn rest(&self) -> &'a [u8] {
        &self.bytes[self.pos..]
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let slice = self.bytes.get(self.pos..self.pos + count)?;
        self.pos += count;
        Some(slice)
    }

    fn varint(&mut self) -> Option<i32> {
        let mut value: i32 = 0;
        for shift in (0..35).step_by(7) {
            let byte = self.u8()?;
            value |= i32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn u8(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn i8(&mut self) -> Option<i8> {
        self.u8().map(|byte| byte as i8)
    }

    fn i16(&mut self) -> Option<i16> {
        self.take(2)
            .map(|b| i16::from_be_bytes(b.try_into().unwrap()))
    }

    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_be_bytes(b.try_into().unwrap()))
    }

    fn f32(&mut self) -> Option<f32> {
        self.take(4)
            .map(|b| f32::from_be_bytes(b.try_into().unwrap()))
    }

    fn f64(&mut self) -> Option<f64> {
        self.take(8)
            .map(|b| f64::from_be_bytes(b.try_into().unwrap()))
    }

    fn uuid(&mut self) -> Option<String> {
        let bytes = self.take(16)?;
        Some(format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
            bytes[4],
            bytes[5],
            bytes[6],
            bytes[7],
            bytes[8],
            bytes[9],
            bytes[10],
            bytes[11],
            bytes[12],
            bytes[13],
            bytes[14],
            bytes[15]
        ))
    }

    fn string(&mut self) -> Option<String> {
        let length = usize::try_from(self.varint()?).ok()?;
        let bytes = self.take(length)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// An opened recording plus its playhead.
pub struct ReplaySession {
    replay: Box<dyn Replay>,
    state: Arc<Mutex<ReplayState>>,
    position_ms: u32,
    next: usize,
}

impl ReplaySession {
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        let replay = replayer::open(bytes).map_err(|error| error.to_string())?;
        let state = ReplayState::new(replay.metadata());
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            replay,
            position_ms: 0,
            next: 0,
        })
    }

    pub fn metadata(&self) -> &Metadata {
        self.replay.metadata()
    }

    pub fn position_ms(&self) -> u32 {
        self.position_ms
    }

    pub fn state(&self) -> Arc<Mutex<ReplayState>> {
        Arc::clone(&self.state)
    }

    pub fn entities(&self) -> Vec<EntityInfo> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entities()
    }

    /// Moves the playhead, applying packets to the shared state.
    pub fn apply_until(&mut self, target_ms: u32) {
        let packets = self.replay.packets();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if target_ms < self.position_ms {
            state.reset();
            self.next = 0;
        }
        while let Some(packet) = packets.get(self.next) {
            if packet.time_ms > target_ms {
                break;
            }
            state.apply(&packet.data);
            self.next += 1;
        }
        self.position_ms = target_ms;
    }
}

/// The viewer's source for a replay: registry data comes from a
/// generated world of the same game version; chunk bodies come from
/// the recording.
pub struct ReplayWorld {
    base: SteelWorld,
    state: Arc<Mutex<ReplayState>>,
}

impl ReplayWorld {
    pub fn new(base: SteelWorld, state: Arc<Mutex<ReplayState>>) -> Self {
        Self { base, state }
    }
}

impl WorldSource for ReplayWorld {
    fn dimension(&self) -> Dimension {
        self.base.dimension()
    }

    fn biome_count(&self) -> usize {
        self.base.biome_count()
    }

    fn biome_climate(&self) -> HashMap<u32, BiomeClimate> {
        self.base.biome_climate()
    }

    fn environment(&self) -> Environment {
        self.base.environment()
    }

    fn chunk_packet(&self, x: i32, z: i32) -> Result<Vec<u8>, String> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .chunks
            .get(&(x, z))
            .cloned()
            .ok_or_else(|| "the replay has not sent this chunk".to_owned())
    }
}
