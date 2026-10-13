use alloc::vec::Vec;

/// A rejected navigation path mutation. Rejection preserves the path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationError {
    /// The prepared number of destinations has been reached.
    Capacity { capacity: usize },
    /// No further distinct path revisions can be represented.
    RevisionExhausted,
}

/// An observable path of pushed destinations, excluding the root page.
///
/// Register through `app.add_model(path).finish()` or store as an owned Model field. Mutations reuse the capacity prepared by [`Self::with_capacity`]; equal destinations remain distinct entries.
#[crate::model]
pub struct NavigationPath<R: Clone + 'static> {
    pub(super) entries: Vec<(u64, R)>,
    capacity: usize,
    revision: u64,
}

#[crate::model]
impl<R: Clone + 'static> NavigationPath<R> {
    /// Prepare storage for at most `capacity` pushed destinations.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            capacity,
            revision: 0,
        }
    }

    /// Number of pushed destinations; the root page is not counted.
    #[observe]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the path selects its root page.
    #[observe]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Prepared limit on pushed destinations.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Revision of the last accepted structural change.
    #[observe]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Push an owned destination without growing storage or cloning its value.
    pub fn push(&mut self, route: R) -> Result<(), NavigationError> {
        if self.entries.len() == self.capacity {
            return Err(NavigationError::Capacity {
                capacity: self.capacity,
            });
        }
        self.advance_revision()?;
        self.entries.push((self.revision, route));
        Ok(())
    }

    /// Remove the last destination. An empty path is unchanged.
    pub fn pop(&mut self) -> Result<(), NavigationError> {
        if self.entries.is_empty() {
            return Ok(());
        }
        self.advance_revision()?;
        self.entries.pop();
        Ok(())
    }

    /// Return to the root page, retaining the prepared path storage.
    pub fn clear(&mut self) -> Result<(), NavigationError> {
        if self.entries.is_empty() {
            return Ok(());
        }
        self.advance_revision()?;
        self.entries.clear();
        Ok(())
    }

    #[model(local)]
    fn advance_revision(&mut self) -> Result<(), NavigationError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(NavigationError::RevisionExhausted)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
