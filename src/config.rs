use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer};
use smithay_client_toolkit::shell::wlr_layer::Layer;

const CONFIG_RELATIVE_PATH: &str = "selector/config.toml";

// straight (non-premultiplied) 8-bit rgba
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn is_transparent(&self) -> bool {
        self.a == 0
    }

    // #rrggbb or #rrggbbaa, the leading hash is optional and alpha defaults to opaque
    pub fn from_hex(text: &str) -> Result<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);

        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("`{text}` is not a hex colour");
        }

        let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).unwrap_or(0);

        match digits.len() {
            6 => Ok(Self::rgba(channel(0), channel(2), channel(4), 0xff)),
            8 => Ok(Self::rgba(channel(0), channel(2), channel(4), channel(6))),
            _ => bail!("`{text}` must be 6 or 8 hex digits, as #rrggbb or #rrggbbaa"),
        }
    }

    // wl_shm Argb8888 is little-endian and premultiplied, so scale the channels by alpha once here rather than at every pixel write
    pub fn to_argb8888(self) -> [u8; 4] {
        let premultiply = |c: u8| ((c as u32 * self.a as u32 + 127) / 255) as u8;
        [
            premultiply(self.b),
            premultiply(self.g),
            premultiply(self.r),
            self.a,
        ]
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Color::from_hex(&text).map_err(serde::de::Error::custom)
    }
}

// Layer is a foreign type, so it cannot derive Deserialize
fn deserialize_layer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Layer, D::Error> {
    let text = String::deserialize(deserializer)?;
    match text.as_str() {
        "background" => Ok(Layer::Background),
        "bottom" => Ok(Layer::Bottom),
        "top" => Ok(Layer::Top),
        "overlay" => Ok(Layer::Overlay),
        other => Err(serde::de::Error::custom(format!(
            "`{other}` is not a layer, expected background, bottom, top or overlay"
        ))),
    }
}

// deny_unknown_fields so a typo is an error rather than a setting that silently never applies
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub fill: Color,
    pub border: Color,
    // zero disables the outline
    pub border_width: u32,
    // Bottom puts us above the wallpaper (Background) but below every ordinary window,
    // so drags land here only when the desktop underneath is empty
    #[serde(deserialize_with = "deserialize_layer")]
    pub layer: Layer,
    // how far the pointer must travel before a press counts as a drag, without it a plain click flashes a one-pixel rectangle
    pub drag_threshold: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fill: Color::rgba(0x4c, 0x9e, 0xd9, 0x40),
            border: Color::rgba(0x4c, 0x9e, 0xd9, 0xcc),
            border_width: 1,
            layer: Layer::Bottom,
            drag_threshold: 3.0,
        }
    }
}

impl Config {
    // a missing config is fine and means defaults, an unreadable or malformed one is not:
    // falling back silently would leave the user with no way to tell their config never took effect
    pub fn load() -> Result<Self> {
        let Some(path) = config_path() else {
            log::debug!("neither XDG_CONFIG_HOME nor HOME is set, using defaults");
            return Ok(Self::default());
        };

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                log::debug!("no config at {}, using defaults", path.display());
                return Ok(Self::default());
            }
            Err(err) => {
                return Err(err).with_context(|| format!("could not read {}", path.display()));
            }
        };

        let config = toml::from_str(&text)
            .with_context(|| format!("invalid config at {}", path.display()))?;

        log::info!("loaded config from {}", path.display());
        Ok(config)
    }
}

fn config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME")
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir).join(CONFIG_RELATIVE_PATH));
    }

    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join(CONFIG_RELATIVE_PATH),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_colors_survive_premultiplication() {
        let argb = Color::rgba(0x12, 0x34, 0x56, 0xff).to_argb8888();
        assert_eq!(argb, [0x56, 0x34, 0x12, 0xff]);
    }

    #[test]
    fn half_alpha_scales_the_color_channels() {
        let argb = Color::rgba(0xff, 0xff, 0xff, 0x80).to_argb8888();
        assert_eq!(argb, [0x80, 0x80, 0x80, 0x80]);
    }

    #[test]
    fn zero_alpha_clears_every_channel() {
        assert_eq!(Color::rgba(0xff, 0xff, 0xff, 0).to_argb8888(), [0, 0, 0, 0]);
    }

    #[test]
    fn hex_parses_with_and_without_alpha() {
        assert_eq!(
            Color::from_hex("#4c9ed9").unwrap(),
            Color::rgba(0x4c, 0x9e, 0xd9, 0xff)
        );
        assert_eq!(
            Color::from_hex("#4c9ed940").unwrap(),
            Color::rgba(0x4c, 0x9e, 0xd9, 0x40)
        );
        assert_eq!(
            Color::from_hex("4c9ed9").unwrap(),
            Color::rgba(0x4c, 0x9e, 0xd9, 0xff)
        );
    }

    #[test]
    fn hex_rejects_bad_input() {
        assert!(Color::from_hex("#abc").is_err(), "wrong digit count");
        assert!(Color::from_hex("#gggggg").is_err(), "not hex digits");
        assert!(Color::from_hex("").is_err(), "empty");
    }

    #[test]
    fn an_empty_config_is_all_defaults() {
        let config: Config = toml::from_str("").unwrap();
        let default = Config::default();

        assert_eq!(config.fill, default.fill);
        assert_eq!(config.border, default.border);
        assert_eq!(config.border_width, default.border_width);
        assert_eq!(config.layer, default.layer);
        assert_eq!(config.drag_threshold, default.drag_threshold);
    }

    #[test]
    fn a_partial_config_keeps_defaults_for_everything_else() {
        let config: Config = toml::from_str(r#"border_width = 4"#).unwrap();

        assert_eq!(config.border_width, 4);
        assert_eq!(config.fill, Config::default().fill);
    }

    #[test]
    fn every_layer_name_parses() {
        for (name, expected) in [
            ("background", Layer::Background),
            ("bottom", Layer::Bottom),
            ("top", Layer::Top),
            ("overlay", Layer::Overlay),
        ] {
            let config: Config = toml::from_str(&format!("layer = \"{name}\"")).unwrap();
            assert_eq!(config.layer, expected);
        }
    }

    #[test]
    fn an_unknown_key_is_an_error() {
        assert!(toml::from_str::<Config>("bordr_width = 1").is_err());
    }

    #[test]
    fn an_unknown_layer_is_an_error() {
        assert!(toml::from_str::<Config>(r#"layer = "middle""#).is_err());
    }

    // keeps the shipped example honest, it must stay parseable and match the defaults it documents
    #[test]
    fn the_example_config_parses_and_matches_the_defaults() {
        let example = include_str!("../config.example.toml");
        let config: Config = toml::from_str(example).expect("example config must parse");
        let default = Config::default();

        assert_eq!(config.fill, default.fill);
        assert_eq!(config.border, default.border);
        assert_eq!(config.border_width, default.border_width);
        assert_eq!(config.layer, default.layer);
        assert_eq!(config.drag_threshold, default.drag_threshold);
    }
}
