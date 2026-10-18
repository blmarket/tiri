//! Owned Unicode caches. Defaults remain static and dynamic items stay boxed.
#![forbid(unsafe_code)]

use crate::shared::utf8::wchar_t;
use std::collections::BTreeMap;
use std::sync::Mutex;

struct WidthItem {
    wc: wchar_t,
    width: u32,
}

enum WidthEntry {
    Default(&'static WidthItem),
    Owned(Box<WidthItem>),
}

pub(super) struct Utf8WidthCache {
    entries: BTreeMap<wchar_t, WidthEntry>,
}

impl Utf8WidthCache {
    pub(super) const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub(super) fn reset_defaults(&mut self) {
        self.entries.clear();
        for item in &DEFAULT_WIDTHS {
            self.entries
                .entry(item.wc)
                .or_insert(WidthEntry::Default(item));
        }
    }

    pub(super) fn insert(&mut self, wc: wchar_t, width: u32) {
        self.entries
            .insert(wc, WidthEntry::Owned(Box::new(WidthItem { wc, width })));
    }

    pub(super) fn find(&self, wc: wchar_t) -> Option<u32> {
        self.entries.get(&wc).map(|entry| match entry {
            WidthEntry::Default(item) => item.width,
            WidthEntry::Owned(item) => item.width,
        })
    }

    #[cfg(test)]
    pub(super) fn remove(&mut self, wc: wchar_t) {
        self.entries.remove(&wc);
    }
}

pub(super) struct Utf8ItemCache {
    by_data: BTreeMap<(u8, Vec<u8>), u32>,
    by_index: BTreeMap<u32, Box<[u8; 32]>>,
    next_index: u32,
}

impl Utf8ItemCache {
    pub(super) const fn new() -> Self {
        Self {
            by_data: BTreeMap::new(),
            by_index: BTreeMap::new(),
            next_index: 0,
        }
    }

    /// Return the stable 24-bit index and whether this insertion was new.
    pub(super) fn intern(&mut self, data: &[u8]) -> Option<(u32, bool)> {
        if data.len() > 32 {
            return None;
        }
        let key = (data.len() as u8, data.to_vec());
        if let Some(&index) = self.by_data.get(&key) {
            return Some((index, false));
        }
        if self.next_index == 0x1000000 {
            return None;
        }
        let mut item = Box::new([0; 32]);
        item[..data.len()].copy_from_slice(data);
        let index = self.next_index;
        self.next_index += 1;
        self.by_index.insert(index, item);
        self.by_data.insert(key, index);
        Some((index, true))
    }

    pub(super) fn get(&self, index: u32) -> Option<&[u8; 32]> {
        self.by_index.get(&index).map(Box::as_ref)
    }
}

pub(super) static UTF8_WIDTHS: Mutex<Utf8WidthCache> = Mutex::new(Utf8WidthCache::new());
pub(super) static UTF8_ITEMS: Mutex<Utf8ItemCache> = Mutex::new(Utf8ItemCache::new());

static DEFAULT_WIDTHS: [WidthItem; 162] = [
    WidthItem {
        wc: 0x261d,
        width: 2,
    },
    WidthItem {
        wc: 0x26f9,
        width: 2,
    },
    WidthItem {
        wc: 0x270a,
        width: 2,
    },
    WidthItem {
        wc: 0x270b,
        width: 2,
    },
    WidthItem {
        wc: 0x270c,
        width: 2,
    },
    WidthItem {
        wc: 0x270d,
        width: 2,
    },
    WidthItem {
        wc: 0x1f1e6,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1e7,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1e8,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1e9,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1ea,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1eb,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1ec,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1ed,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1ee,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1ef,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f0,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f1,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f2,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f3,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f4,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f5,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f6,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f7,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f8,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1f9,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1fa,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1fb,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1fc,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1fd,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1fe,
        width: 1,
    },
    WidthItem {
        wc: 0x1f1ff,
        width: 1,
    },
    WidthItem {
        wc: 0x1f385,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3c2,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3c3,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3c4,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3c7,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3ca,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3cb,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3cc,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3fb,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3fc,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3fd,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3fe,
        width: 2,
    },
    WidthItem {
        wc: 0x1f3ff,
        width: 2,
    },
    WidthItem {
        wc: 0x1f442,
        width: 2,
    },
    WidthItem {
        wc: 0x1f443,
        width: 2,
    },
    WidthItem {
        wc: 0x1f446,
        width: 2,
    },
    WidthItem {
        wc: 0x1f447,
        width: 2,
    },
    WidthItem {
        wc: 0x1f448,
        width: 2,
    },
    WidthItem {
        wc: 0x1f449,
        width: 2,
    },
    WidthItem {
        wc: 0x1f44a,
        width: 2,
    },
    WidthItem {
        wc: 0x1f44b,
        width: 2,
    },
    WidthItem {
        wc: 0x1f44c,
        width: 2,
    },
    WidthItem {
        wc: 0x1f44d,
        width: 2,
    },
    WidthItem {
        wc: 0x1f44e,
        width: 2,
    },
    WidthItem {
        wc: 0x1f44f,
        width: 2,
    },
    WidthItem {
        wc: 0x1f450,
        width: 2,
    },
    WidthItem {
        wc: 0x1f466,
        width: 2,
    },
    WidthItem {
        wc: 0x1f467,
        width: 2,
    },
    WidthItem {
        wc: 0x1f468,
        width: 2,
    },
    WidthItem {
        wc: 0x1f469,
        width: 2,
    },
    WidthItem {
        wc: 0x1f46b,
        width: 2,
    },
    WidthItem {
        wc: 0x1f46c,
        width: 2,
    },
    WidthItem {
        wc: 0x1f46d,
        width: 2,
    },
    WidthItem {
        wc: 0x1f46e,
        width: 2,
    },
    WidthItem {
        wc: 0x1f470,
        width: 2,
    },
    WidthItem {
        wc: 0x1f471,
        width: 2,
    },
    WidthItem {
        wc: 0x1f472,
        width: 2,
    },
    WidthItem {
        wc: 0x1f473,
        width: 2,
    },
    WidthItem {
        wc: 0x1f474,
        width: 2,
    },
    WidthItem {
        wc: 0x1f475,
        width: 2,
    },
    WidthItem {
        wc: 0x1f476,
        width: 2,
    },
    WidthItem {
        wc: 0x1f477,
        width: 2,
    },
    WidthItem {
        wc: 0x1f478,
        width: 2,
    },
    WidthItem {
        wc: 0x1f47c,
        width: 2,
    },
    WidthItem {
        wc: 0x1f481,
        width: 2,
    },
    WidthItem {
        wc: 0x1f482,
        width: 2,
    },
    WidthItem {
        wc: 0x1f483,
        width: 2,
    },
    WidthItem {
        wc: 0x1f485,
        width: 2,
    },
    WidthItem {
        wc: 0x1f486,
        width: 2,
    },
    WidthItem {
        wc: 0x1f487,
        width: 2,
    },
    WidthItem {
        wc: 0x1f48f,
        width: 2,
    },
    WidthItem {
        wc: 0x1f491,
        width: 2,
    },
    WidthItem {
        wc: 0x1f4aa,
        width: 2,
    },
    WidthItem {
        wc: 0x1f574,
        width: 2,
    },
    WidthItem {
        wc: 0x1f575,
        width: 2,
    },
    WidthItem {
        wc: 0x1f57a,
        width: 2,
    },
    WidthItem {
        wc: 0x1f590,
        width: 2,
    },
    WidthItem {
        wc: 0x1f595,
        width: 2,
    },
    WidthItem {
        wc: 0x1f596,
        width: 2,
    },
    WidthItem {
        wc: 0x1f645,
        width: 2,
    },
    WidthItem {
        wc: 0x1f646,
        width: 2,
    },
    WidthItem {
        wc: 0x1f647,
        width: 2,
    },
    WidthItem {
        wc: 0x1f64b,
        width: 2,
    },
    WidthItem {
        wc: 0x1f64c,
        width: 2,
    },
    WidthItem {
        wc: 0x1f64d,
        width: 2,
    },
    WidthItem {
        wc: 0x1f64e,
        width: 2,
    },
    WidthItem {
        wc: 0x1f64f,
        width: 2,
    },
    WidthItem {
        wc: 0x1f6a3,
        width: 2,
    },
    WidthItem {
        wc: 0x1f6b4,
        width: 2,
    },
    WidthItem {
        wc: 0x1f6b5,
        width: 2,
    },
    WidthItem {
        wc: 0x1f6b6,
        width: 2,
    },
    WidthItem {
        wc: 0x1f6c0,
        width: 2,
    },
    WidthItem {
        wc: 0x1f6cc,
        width: 2,
    },
    WidthItem {
        wc: 0x1f90c,
        width: 2,
    },
    WidthItem {
        wc: 0x1f90f,
        width: 2,
    },
    WidthItem {
        wc: 0x1f918,
        width: 2,
    },
    WidthItem {
        wc: 0x1f919,
        width: 2,
    },
    WidthItem {
        wc: 0x1f91a,
        width: 2,
    },
    WidthItem {
        wc: 0x1f91b,
        width: 2,
    },
    WidthItem {
        wc: 0x1f91c,
        width: 2,
    },
    WidthItem {
        wc: 0x1f91d,
        width: 2,
    },
    WidthItem {
        wc: 0x1f91e,
        width: 2,
    },
    WidthItem {
        wc: 0x1f91f,
        width: 2,
    },
    WidthItem {
        wc: 0x1f926,
        width: 2,
    },
    WidthItem {
        wc: 0x1f930,
        width: 2,
    },
    WidthItem {
        wc: 0x1f931,
        width: 2,
    },
    WidthItem {
        wc: 0x1f932,
        width: 2,
    },
    WidthItem {
        wc: 0x1f933,
        width: 2,
    },
    WidthItem {
        wc: 0x1f934,
        width: 2,
    },
    WidthItem {
        wc: 0x1f935,
        width: 2,
    },
    WidthItem {
        wc: 0x1f936,
        width: 2,
    },
    WidthItem {
        wc: 0x1f937,
        width: 2,
    },
    WidthItem {
        wc: 0x1f938,
        width: 2,
    },
    WidthItem {
        wc: 0x1f939,
        width: 2,
    },
    WidthItem {
        wc: 0x1f93d,
        width: 2,
    },
    WidthItem {
        wc: 0x1f93e,
        width: 2,
    },
    WidthItem {
        wc: 0x1f977,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9b5,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9b6,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9b8,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9b9,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9bb,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9cd,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9ce,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9cf,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d1,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d2,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d3,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d4,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d5,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d6,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d7,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d8,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9d9,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9da,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9db,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9dc,
        width: 2,
    },
    WidthItem {
        wc: 0x1f9dd,
        width: 2,
    },
    WidthItem {
        wc: 0x1fac3,
        width: 2,
    },
    WidthItem {
        wc: 0x1fac4,
        width: 2,
    },
    WidthItem {
        wc: 0x1fac5,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf0,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf1,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf2,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf3,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf4,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf5,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf6,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf7,
        width: 2,
    },
    WidthItem {
        wc: 0x1faf8,
        width: 2,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_indexes_deduplicate_exact_bytes_and_preserve_zero_padding() {
        let mut cache = Utf8ItemCache::new();
        for (expected, data) in [&b"\x80"[..], &b"\xff"[..], &b"\0\0"[..], &b"abcd"[..]]
            .into_iter()
            .enumerate()
        {
            let index = expected as u32;
            assert_eq!(cache.intern(data), Some((index, true)));
            assert_eq!(cache.intern(data), Some((index, false)));
            let item = cache.get(index).unwrap();
            assert_eq!(&item[..data.len()], data);
            assert!(item[data.len()..].iter().all(|&byte| byte == 0));
        }
        assert_eq!(cache.intern(b"abcd\0"), Some((4, true)));
        assert_eq!(
            cache.by_data.keys().cloned().collect::<Vec<_>>(),
            vec![
                (1, vec![0x80]),
                (1, vec![0xff]),
                (2, vec![0, 0]),
                (4, b"abcd".to_vec()),
                (5, b"abcd\0".to_vec()),
            ]
        );
        assert!(cache.get(5).is_none());
        assert!(cache.intern(&[0; 33]).is_none());
        assert_eq!(cache.intern(b"next"), Some((5, true)));
    }

    #[test]
    fn exhausted_indexes_retain_existing_characters() {
        let mut cache = Utf8ItemCache::new();
        assert_eq!(cache.intern(b"existing"), Some((0, true)));
        cache.next_index = 0xffffff;
        assert_eq!(cache.intern(b"last"), Some((0xffffff, true)));
        assert!(cache.intern(b"new").is_none());
        assert_eq!(cache.intern(b"existing"), Some((0, false)));
        assert_eq!(cache.intern(b"last"), Some((0xffffff, false)));
        assert_eq!(&cache.get(0xffffff).unwrap()[..4], b"last");
    }

    #[test]
    fn width_rebuild_drops_overrides_and_reuses_static_defaults() {
        let mut cache = Utf8WidthCache::new();
        assert!(cache.find(0x261d).is_none());
        for _ in 0..32 {
            cache.reset_defaults();
            assert_eq!(cache.find(0x261d), Some(2));
            assert!(matches!(
                cache.entries.get(&0x261d),
                Some(WidthEntry::Default(_))
            ));
            assert!(cache.find(0xe010).is_none());
            cache.insert(0x261d, 0);
            cache.insert(0xe010, 2);
            assert_eq!(cache.find(0x261d), Some(0));
            cache.insert(0x261d, 1);
            assert_eq!(cache.find(0x261d), Some(1));
            assert!(matches!(
                cache.entries.get(&0x261d),
                Some(WidthEntry::Owned(_))
            ));
            assert_eq!(cache.find(0xe010), Some(2));
        }
    }
}
