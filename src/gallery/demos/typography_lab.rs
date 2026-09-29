extern crate alloc;

#[cfg(any(feature = "std", test))]
mod caret;
#[cfg(any(feature = "std", test))]
mod composition;
#[cfg(any(feature = "std", test))]
mod contour;
mod runtime;
#[cfg(any(feature = "std", test))]
mod state;
#[cfg(any(feature = "std", test))]
mod style;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
pub use composition::setup_app;
pub(crate) use runtime::register_fonts;

pub const VIEWPORT: (u16, u16) = (1024, 720);

const UI_FONT: &[u8] = include_bytes!("assets/misans_ui.mirx");
const CJK_FONT: &[u8] = include_bytes!("assets/typography_cjk.mirx");
const ARABIC_FONT: &[u8] = include_bytes!("assets/typography_arabic.mirx");
const DEVANAGARI_FONT: &[u8] = include_bytes!("assets/typography_devanagari.mirx");
const THAI_FONT: &[u8] = include_bytes!("assets/typography_thai.mirx");
const ELLIPSIS_FONT: &[u8] = include_bytes!("assets/typography_ellipsis.mirx");

pub const DEMO_SIZE: crate::gallery::DemoSize =
    crate::gallery::DemoSize::range(320, 320, 1024, 720);
