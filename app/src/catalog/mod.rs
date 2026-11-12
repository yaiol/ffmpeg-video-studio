//! The yaiol UI catalog, Dioxus implementation.
//!
//! One catalog, two implementations: the CSS in `assets/` is the shared contract (copied from the
//! shared catalog, never edited here), and each control below emits the SAME class names and
//! element structure as its JSX twin in the shared catalog. Only the controls this app uses are
//! ported; the rest are `pending` — see the project's CLAUDE.md for the status table.

mod pkr_color;
mod cbo_standard;
mod fld_labelled;
mod filedrop;
mod cbo_filter;
mod crop_rotate;
mod curve;
mod bar_window;
mod header;
mod icons;
mod num;
#[allow(dead_code)] // the shared file also serves controls this app does not have
mod popover;
pub mod news;
mod dlg_news;
mod news_loader;
mod markdown_renderer;
mod stg_view;
mod equalizer;
mod transfer;
mod yrgb;

pub use pkr_color::UisPkrColor;
pub use cbo_standard::{UisCboStandard, UisComboItem};
pub use fld_labelled::UisFldLabelled;
pub use crop_rotate::UisCropRotate;
pub use curve::{CurveChannel, CurvePoints, UisCurve, straight};
pub use filedrop::LbsDropZone;
pub use cbo_filter::{UisCboCategory, UisCboFilter};
pub use bar_window::UisBarWindow;
pub use header::UisAppHeader;
pub use icons::UisIcoGithub;
pub use num::UisNum;
pub use news_loader::UisNewsLoader;
pub use stg_view::{UisAppTab, UisStgView};
pub use equalizer::{EqView, UisEqualizer};
pub use transfer::UisTransfer;
pub use yrgb::{UisYrgb, YrgbBar, YrgbView};
