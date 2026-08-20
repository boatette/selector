use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer};
use smithay_client_toolkit::shell::wlr_layer::Layer;

const CONFIG_RELATIVE_PATH: &str = "selector/config.toml";

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

    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }

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

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub fill: Color,
    pub border: Color,
    pub border_width: u32,
    pub corner_radius: u32,
    #[serde(deserialize_with = "deserialize_layer")]
    pub layer: Layer,
    pub drag_threshold: f64,
    pub blur: bool,
    pub colors: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Colors {
    fill: Option<Color>,
    border: Option<Color>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fill: Color::rgba(0x4c, 0x9e, 0xd9, 0x40),
            border: Color::rgba(0x4c, 0x9e, 0xd9, 0xcc),
            border_width: 1,
            corner_radius: 0,
            layer: Layer::Bottom,
            drag_threshold: 3.0,
            blur: false,
            colors: None,
        }
    }
}

impl Config {
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

        let mut config: Self = toml::from_str(&text)
            .with_context(|| format!("invalid config at {}", path.display()))?;

        log::info!("loaded config from {}", path.display());

        let base = path.parent().unwrap_or_else(|| Path::new(""));
        config.load_colors(base, home_dir().as_deref())?;

        Ok(config)
    }

    fn load_colors(&mut self, base: &Path, home: Option<&Path>) -> Result<()> {
        let Some(configured) = self.colors.clone() else {
            return Ok(());
        };
        let path = resolve_path(&configured, base, home);

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                log::warn!(
                    "no colour file at {}, keeping the colours from the main config",
                    path.display()
                );
                return Ok(());
            }
            Err(err) => {
                return Err(err).with_context(|| format!("could not read {}", path.display()));
            }
        };

        let colors: Colors = toml::from_str(&text)
            .with_context(|| format!("invalid colour file at {}", path.display()))?;

        self.apply_colors(colors);
        log::info!("loaded colours from {}", path.display());
        Ok(())
    }

    fn apply_colors(&mut self, colors: Colors) {
        if let Some(fill) = colors.fill {
            self.fill = fill;
        }
        if let Some(border) = colors.border {
            self.border = border;
        }
    }
}

fn resolve_path(path: &Path, base: &Path, home: Option<&Path>) -> PathBuf {
    let expanded = match (path.strip_prefix("~"), home) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path.to_path_buf(),
    };

    if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
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
        assert_eq!(config.corner_radius, default.corner_radius);
        assert_eq!(config.layer, default.layer);
        assert_eq!(config.drag_threshold, default.drag_threshold);
    }

    #[test]
    fn a_partial_config_keeps_defaults_for_everything_else() {
        let config: Config = toml::from_str("border_width = 4").unwrap();

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

    fn colors_from(text: &str) -> Colors {
        toml::from_str(text).unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("selector-test-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_colour_file_overrides_the_main_config() {
        let mut config = Config::default();
        config.apply_colors(colors_from(
            r##"
            fill = "#11223344"
            border = "#556677"
            "##,
        ));

        assert_eq!(config.fill, Color::rgba(0x11, 0x22, 0x33, 0x44));
        assert_eq!(config.border, Color::rgba(0x55, 0x66, 0x77, 0xff));
    }

    #[test]
    fn a_partial_colour_file_leaves_the_other_colour_alone() {
        let mut config = Config::default();
        let untouched = config.border;
        config.apply_colors(colors_from(r##"fill = "#11223344""##));

        assert_eq!(config.fill, Color::rgba(0x11, 0x22, 0x33, 0x44));
        assert_eq!(config.border, untouched);
    }

    #[test]
    fn an_empty_colour_file_changes_nothing() {
        let mut config = Config::default();
        config.apply_colors(colors_from(""));

        assert_eq!(config.fill, Config::default().fill);
        assert_eq!(config.border, Config::default().border);
    }

    #[test]
    fn a_colour_file_may_not_carry_other_settings() {
        assert!(
            toml::from_str::<Colors>("border_width = 4").is_err(),
            "the overlay is colours only"
        );
    }

    #[test]
    fn a_missing_colour_file_is_not_an_error() {
        let dir = temp_dir("missing");
        let mut config = Config {
            colors: Some(PathBuf::from("nope.toml")),
            ..Config::default()
        };

        config.load_colors(&dir, None).expect("must not fail");
        assert_eq!(
            config.fill,
            Config::default().fill,
            "the main config's colours survive"
        );
    }

    #[test]
    fn a_malformed_colour_file_is_an_error() {
        let dir = temp_dir("malformed");
        fs::write(dir.join("colors.toml"), r#"fill = "not-a-colour""#).unwrap();

        let mut config = Config {
            colors: Some(PathBuf::from("colors.toml")),
            ..Config::default()
        };

        assert!(config.load_colors(&dir, None).is_err());
    }

    #[test]
    fn a_relative_colour_path_resolves_against_the_config_directory() {
        let dir = temp_dir("relative");
        fs::write(dir.join("colors.toml"), r##"fill = "#89b4fa40""##).unwrap();

        let mut config = Config {
            colors: Some(PathBuf::from("colors.toml")),
            ..Config::default()
        };

        config.load_colors(&dir, None).unwrap();
        assert_eq!(config.fill, Color::rgba(0x89, 0xb4, 0xfa, 0x40));
    }

    #[test]
    fn paths_expand_tilde_and_resolve_relative_against_the_base() {
        let home = Path::new("/home/someone");
        let base = Path::new("/etc/selector");

        assert_eq!(
            resolve_path(Path::new("~/.config/c.toml"), base, Some(home)),
            PathBuf::from("/home/someone/.config/c.toml")
        );
        assert_eq!(
            resolve_path(Path::new("c.toml"), base, Some(home)),
            PathBuf::from("/etc/selector/c.toml")
        );
        assert_eq!(
            resolve_path(Path::new("/absolute/c.toml"), base, Some(home)),
            PathBuf::from("/absolute/c.toml")
        );
    }

    #[test]
    fn a_tilde_path_is_left_alone_without_a_home() {
        assert_eq!(
            resolve_path(Path::new("~/c.toml"), Path::new("/etc"), None),
            PathBuf::from("/etc/~/c.toml")
        );
    }

    #[test]
    fn the_example_config_parses_and_matches_the_defaults() {
        let example = include_str!("../config.example.toml");
        let config: Config = toml::from_str(example).expect("example config must parse");
        let default = Config::default();

        assert_eq!(config.fill, default.fill);
        assert_eq!(config.border, default.border);
        assert_eq!(config.border_width, default.border_width);
        assert_eq!(config.corner_radius, default.corner_radius);
        assert_eq!(config.layer, default.layer);
        assert_eq!(config.drag_threshold, default.drag_threshold);
    }
}
