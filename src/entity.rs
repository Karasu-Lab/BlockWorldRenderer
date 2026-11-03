//! An entity as the web hooks expose it, whichever session produced it.

/// One entity in a replay or a world save.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityInfo {
    pub id: Option<i32>,
    pub uuid: Option<String>,
    pub name: Option<String>,
    pub is_player: bool,
    pub position: Option<[f64; 3]>,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
}

impl EntityInfo {
    /// The JSON object the TypeScript entity types mirror.
    pub fn to_json(&self) -> String {
        fn string(value: &Option<String>) -> String {
            match value {
                Some(value) => format!("\"{}\"", escape(value)),
                None => "null".to_owned(),
            }
        }
        fn number(value: Option<i32>) -> String {
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into())
        }
        let position = match self.position {
            Some([x, y, z]) => format!("[{x},{y},{z}]"),
            None => "null".to_owned(),
        };
        format!(
            "{{\"id\":{},\"uuid\":{},\"name\":{},\"isPlayer\":{},\"position\":{},\"yaw\":{},\"pitch\":{}}}",
            number(self.id),
            string(&self.uuid),
            string(&self.name),
            self.is_player,
            position,
            self.yaw_deg,
            self.pitch_deg,
        )
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

/// A JSON array of entities.
pub fn entities_json(entities: &[EntityInfo]) -> String {
    let items: Vec<String> = entities.iter().map(EntityInfo::to_json).collect();
    format!("[{}]", items.join(","))
}
