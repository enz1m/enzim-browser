//! Stable page identities for tabs.
//!
//! A tab's key is replaced whenever it starts another document, so that late
//! results for the old document are recognised. Agents need a name for the tab
//! that survives navigation and moves between windows: the key the tab had
//! first. Keys come from one process-wide counter, so a process-wide map from
//! the current key to that first key names each tab uniquely. Only renamed
//! tabs have an entry; an unrenamed tab's page id is its key.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

static RENAMED: Mutex<Option<HashMap<u64, u64>>> = Mutex::new(None);

/// Records that the tab keyed `old` is now keyed `new`.
pub(crate) fn rename(old: u64, new: u64) {
    if old == new {
        return;
    }
    let mut map = RENAMED.lock().unwrap_or_else(PoisonError::into_inner);
    let map = map.get_or_insert_with(HashMap::new);
    let page = map.remove(&old).unwrap_or(old);
    map.insert(new, page);
}

/// The page id of the tab keyed `key`.
pub(crate) fn page_of(key: u64) -> u64 {
    RENAMED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|map| map.get(&key).copied())
        .unwrap_or(key)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_page_keeps_its_id_through_renames() {
        let first = crate::browser::next_tab_key();
        let second = crate::browser::next_tab_key();
        let third = crate::browser::next_tab_key();
        super::rename(first, second);
        super::rename(second, third);
        assert_eq!(super::page_of(third), first, "the first key names the page");
        let other = crate::browser::next_tab_key();
        assert_eq!(super::page_of(other), other, "an unrenamed tab is its key");
    }
}
