//! Spritesheet layout, and the hand-written deserialiser it needs.
//!
//! Out of `manifest.rs` because the manual `Deserialize` is most of its bulk and
//! none of its subject: a manifest's `spritesheet` field arrives as a map, as an
//! empty array, or as null, and only one of those three shapes is what serde
//! derives for a struct.

use serde::{Deserialize, Serialize};

/// Spritesheet layout definition.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SpritesheetConfig {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub frame_width: Option<u32>,
    #[serde(default)]
    pub frame_height: Option<u32>,
    #[serde(default)]
    pub columns: Option<u32>,
    #[serde(default)]
    pub rows: Option<u32>,
    #[serde(default)]
    pub margin_x: Option<u32>,
    #[serde(default)]
    pub margin_y: Option<u32>,
    #[serde(default)]
    pub spacing_x: Option<u32>,
    #[serde(default)]
    pub spacing_y: Option<u32>,
}

impl<'de> Deserialize<'de> for SpritesheetConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SpritesheetVisitor;

        impl<'de> serde::de::Visitor<'de> for SpritesheetVisitor {
            type Value = SpritesheetConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a spritesheet map, empty array, or null")
            }

            fn visit_seq<A>(self, _seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                Ok(SpritesheetConfig::default())
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut cfg = SpritesheetConfig::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "path" => cfg.path = map.next_value()?,
                        "frame_width" => cfg.frame_width = map.next_value()?,
                        "frame_height" => cfg.frame_height = map.next_value()?,
                        "columns" => cfg.columns = map.next_value()?,
                        "rows" => cfg.rows = map.next_value()?,
                        "margin_x" => cfg.margin_x = map.next_value()?,
                        "margin_y" => cfg.margin_y = map.next_value()?,
                        "spacing_x" => cfg.spacing_x = map.next_value()?,
                        "spacing_y" => cfg.spacing_y = map.next_value()?,
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(cfg)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(SpritesheetConfig::default())
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(SpritesheetConfig::default())
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_any(self)
            }
        }

        deserializer.deserialize_any(SpritesheetVisitor)
    }
}
