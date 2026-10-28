//! Feature flags. Release builds always use the defaults; dev builds can change them from
//! the Developer menu, and the choice is kept in the settings table.

use crate::store;
use anyhow::Result;

/// Debug builds get the Developer menu.
pub const DEVELOPER_BUILD: bool = cfg!(debug_assertions);

const FEEDLY_KEY: &str = "flags.feedly";

/// Feedly sign-in and sync. Off until Feedly support is finished.
pub fn feedly() -> bool {
    DEVELOPER_BUILD && store::get().setting(FEEDLY_KEY).as_deref() == Some("1")
}

pub fn set_feedly(enabled: bool) -> Result<()> {
    store::get().set_setting(FEEDLY_KEY, if enabled { "1" } else { "0" })
}
