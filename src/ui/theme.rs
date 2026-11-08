//! Visual palette and layout constants for the Legenda GPUI interface.

use gpui::{Hsla, hsla};

/// Main window and surface background (`#17181b`).
pub const BG_APP: u32 = 0x17181b;
/// Text input background (`#1b1c20`).
pub const BG_INPUT: u32 = 0x1b1c20;
/// Secondary button background (`#1d1f23`).
pub const BG_BUTTON: u32 = 0x1d1f23;
/// Secondary button hover background (`#292b31`).
pub const BG_BUTTON_HOVER: u32 = 0x292b31;
/// Subtitle result row hover background (`#202227`).
pub const BG_ROW_HOVER: u32 = 0x202227;
/// Selected subtitle result row background (`#302d39`).
pub const BG_ROW_SELECTED: u32 = 0x302d39;

/// Primary accent color used for primary buttons, cursor, and focus ring (`#ded8ee`).
pub const ACCENT: u32 = 0xded8ee;
/// Primary accent hover color (`#eee9f8`).
pub const ACCENT_HOVER: u32 = 0xeee9f8;
/// Dark foreground text color on primary accent surfaces (`#24212c`).
pub const ACCENT_TEXT: u32 = 0x24212c;
/// Translucent text selection highlight (`#ded8ee38`).
pub const SELECTION_BG: u32 = 0xded8ee38;

/// Primary readable text color (`#ececf0`).
pub const TEXT_PRIMARY: u32 = 0xececf0;
/// Secondary/metadata text color (`#a2a4ad`).
pub const TEXT_MUTED: u32 = 0xa2a4ad;
/// Input placeholder text color (`#767882`).
pub const TEXT_PLACEHOLDER: u32 = 0x767882;

/// Subtle divider and container border color (`#292b30`).
pub const BORDER_SUBTLE: u32 = 0x292b30;
/// Default interactive control border color (`#35373d`).
pub const BORDER_DEFAULT: u32 = 0x35373d;
/// Hover border color for secondary controls (`#484a52`).
pub const BORDER_HOVER: u32 = 0x484a52;

/// Toast notification surface background (`#202227`).
pub const BG_TOAST: u32 = 0x202227;
/// Accent stripe for successful toast notifications (`#8fd19e`).
pub const TOAST_SUCCESS: u32 = 0x8fd19e;
/// Accent stripe for error toast notifications (`#f28b82`).
pub const TOAST_ERROR: u32 = 0xf28b82;

/// Semi-transparent modal backdrop scrim.
pub fn modal_backdrop() -> Hsla {
    hsla(0.0, 0.0, 0.0, 0.65)
}
