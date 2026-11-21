use enzui_webkit::{
    content_filter::ContentFilterId,
    ffi::{content_filters::ContentFilterStore, native_task::NativeTaskError},
};
use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

const CAPACITY: usize = 8;

type Entry = (Weak<crate::filters::Source>, ContentFilterId);

#[derive(Default)]
struct IdentityCache(Vec<Entry>);

impl IdentityCache {
    fn identifier(
        &mut self,
        source: &Arc<crate::filters::Source>,
    ) -> Result<ContentFilterId, NativeTaskError> {
        self.0.retain(|(source, _)| source.strong_count() != 0);
        if let Some(index) = self.0.iter().position(|(known, _)| {
            known
                .upgrade()
                .is_some_and(|known| Arc::ptr_eq(&known, source))
        }) {
            let entry = self.0.remove(index);
            let id = entry.1.clone();
            self.0.push(entry);
            return Ok(id);
        }
        let id = match crate::filters::known_identifier(source) {
            Some(identifier) => identifier,
            None => ContentFilterStore::identifier_for(source)?,
        };
        if self.0.len() == CAPACITY {
            self.0.remove(0);
        }
        // Weak ownership prevents stale allocation reuse without retaining rule sources.
        self.0.push((Arc::downgrade(source), id.clone()));
        Ok(id)
    }
}

pub(crate) fn identifier(
    source: &Arc<crate::filters::Source>,
) -> Result<ContentFilterId, NativeTaskError> {
    thread_local! {
        static CACHE: RefCell<IdentityCache> = RefCell::default();
    }
    CACHE.with(|cache| cache.borrow_mut().identifier(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_and_replacement_cannot_reuse_a_previous_source_identity() {
        let mut cache = IdentityCache::default();
        let mut source = Arc::new("[]".to_owned().into());
        let original = cache.identifier(&source).expect("initial identity");
        assert_eq!(cache.identifier(&Arc::clone(&source)).unwrap(), original);
        assert_eq!(Arc::strong_count(&source), 1);
        Arc::make_mut(&mut source).to_mut().push(' ');
        let modified = cache.identifier(&source).expect("modified identity");
        assert_ne!(modified, original);
        assert_eq!(
            modified,
            ContentFilterStore::identifier_for(&source).unwrap()
        );
        Arc::make_mut(&mut source).to_mut().push('\0');
        assert!(cache.identifier(&source).is_err());
        let replacement = Arc::new("[]".to_owned().into());
        assert_eq!(cache.identifier(&replacement).unwrap(), original);
        let invalid = Arc::new("\0".to_owned().into());
        assert!(cache.identifier(&invalid).is_err());
    }

    #[test]
    fn live_source_count_cannot_make_the_identity_cache_unbounded() {
        let mut cache = IdentityCache::default();
        let sources: Vec<Arc<crate::filters::Source>> = (0..CAPACITY * 2)
            .map(|index| Arc::new(format!("[\"{index}\"]").into()))
            .collect();
        for source in &sources {
            assert_eq!(
                cache.identifier(source).unwrap(),
                ContentFilterStore::identifier_for(source).unwrap()
            );
            assert!(cache.0.len() <= CAPACITY);
        }
        assert_eq!(cache.0.len(), CAPACITY);
        for source in &sources {
            assert_eq!(
                cache.identifier(source).unwrap(),
                ContentFilterStore::identifier_for(source).unwrap()
            );
        }
        drop(sources);
        cache.identifier(&Arc::new("[]".to_owned().into())).unwrap();
        assert_eq!(cache.0.len(), 1);
    }
}
