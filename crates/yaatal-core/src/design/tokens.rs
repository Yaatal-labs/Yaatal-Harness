pub const TERRACOTTA_PRIMARY: &str = "#E07856";
pub const INDIGO_DEEP: &str = "#2D3561";
pub const SAVANNA_GOLD: &str = "#F2A541";
pub const FOREST_GREEN: &str = "#1B4D3E";
pub const SAND_NEUTRAL: &str = "#E8D7C3";
pub const CHARCOAL_BASE: &str = "#1F1F1F";
pub const CLAY_WHITE: &str = "#FAF8F5";
pub const RUST_ACCENT: &str = "#B8563E";

pub const FORTRESS_BROWN: &str = "#1A1412";
pub const DAKAR_GOLD: &str = "#D4A017";
pub const SAND_GRAY: &str = "#A9A9A9";
pub const SUNSET_ORANGE: &str = "#F97316";

pub const FONT_HEADING: &[&str] = &["Space Grotesk", "system-ui", "sans-serif"];
pub const FONT_BODY: &[&str] = &["DM Sans", "Plus Jakarta Sans", "system-ui"];

pub const TOUCH_TARGET_MIN: u32 = 44;
pub const FONT_SIZE_MIN: u32 = 16;
pub const BORDER_RADIUS_CARD: &str = "12px";
pub const BORDER_RADIUS_DIAGONAL: &str = "12px 12px 12px 0px";

pub const ADINKRA_SANKOFA: &str = "\u{27F2}";
pub const ADINKRA_GYE_NYAME: &str = "\u{2727}";
pub const ADINKRA_DWENNIMMEN: &str = "\u{269B}";
pub const ADINKRA_FIHANKRA: &str = "\u{25C8}";
pub const ADINKRA_MPATAPO: &str = "\u{26AF}";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    const _: () = assert!(TOUCH_TARGET_MIN >= 44);

    #[test]
    fn test_colors_are_valid_hex() {
        let colors = [
            TERRACOTTA_PRIMARY,
            INDIGO_DEEP,
            SAVANNA_GOLD,
            FOREST_GREEN,
            SAND_NEUTRAL,
            CHARCOAL_BASE,
            CLAY_WHITE,
            RUST_ACCENT,
        ];
        for color in colors {
            assert!(color.starts_with('#'), "Color {} must start with #", color);
            assert_eq!(color.len(), 7, "Color {} must be 7 chars (#RRGGBB)", color);
        }
    }
}
