use super::LoadContext;
use crate::core::resource::probe::HasProbe;
use core::task::Poll;

/// Outcome of [`Loader::poll_load`] (and [`ProbeLoader::poll_probe`]) when the
/// loader did not return a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadError {
    /// Token doesn't belong to this loader's domain. The manager continues
    /// walking the loader chain.
    NotMine,
    /// The loader claimed the token but failed to produce a value. The
    /// manager marks the token as failed (no retry
    /// until [`crate::core::resource::ResourceManager::clear_failed`] is called).
    Failed(&'static str),
    /// Opening or reading the claimed byte source failed.
    Source(super::AssetError),
}

impl LoadError {
    /// Human-readable failure detail, when this is a [`LoadError::Failed`].
    pub fn message(&self) -> Option<&'static str> {
        match self {
            LoadError::NotMine => None,
            LoadError::Failed(m) => Some(m),
            LoadError::Source(_) => Some("resource source failed"),
        }
    }
}

impl From<super::AssetError> for LoadError {
    fn from(error: super::AssetError) -> Self {
        Self::Source(error)
    }
}

/// Resolves a token into an owned `T`. Implementors return
/// [`LoadError::NotMine`] for tokens outside their domain (so the manager
/// keeps walking its loader chain) and [`LoadError::Failed`] for tokens they
/// claim but cannot produce.
///
/// `Fn(&str) -> Result<T, LoadError>` is automatically a [`Loader<T>`] via a
/// blanket impl, so simple loaders can be passed as closures.
pub trait Loader<T>: 'static {
    /// Prepared once per concurrent invocation slot. Reuse owned scratch in `reset` when possible.
    type State: Default + 'static;

    /// Reject unrelated tokens without decoding. Byte recognition consumes only the supplied prefix.
    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch;
    /// Retain progress in `state` and arrange a wake through `cx.waker()` before returning `Pending`. No reference to `cx` may escape this call.
    fn poll_load(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<T, LoadError>>;
    /// Retire an invocation after completion, failure or invalidation. Any externally owned I/O must remain memory-safe when this state is reset.
    fn reset(&self, state: &mut Self::State) {
        *state = Self::State::default();
    }
}

/// Bounded content recognition. `Need` requests a prefix length, not a full decode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatMatch {
    NoMatch,
    Need(usize),
    Match,
}

#[derive(Clone, Copy, Debug)]
pub enum LoadInput<'a> {
    Token(&'a str),
    Bytes {
        token: &'a str,
        prefix: &'a [u8],
        eof: bool,
    },
}

/// Metadata access without producing the decoded resource. Register this capability with [`super::ResourceManager::add_probed_loader`].
pub trait ProbeLoader<T: HasProbe>: Loader<T> {
    fn poll_probe(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<T::Meta, LoadError>>;
}

impl<T, F> Loader<T> for F
where
    F: Fn(&str) -> Result<T, LoadError> + 'static,
{
    type State = ();

    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        match input {
            LoadInput::Token(_) => FormatMatch::Match,
            LoadInput::Bytes { .. } => FormatMatch::NoMatch,
        }
    }

    fn poll_load(&self, _: &mut (), cx: &mut LoadContext<'_>) -> Poll<Result<T, LoadError>> {
        Poll::Ready(self(cx.token()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cache::HasSize;
    use crate::core::resource::probe::HasProbe;
    use alloc::string::{String, ToString};

    #[derive(Clone, Debug, PartialEq)]
    struct Img {
        data: String,
        w: u16,
        h: u16,
    }

    impl HasSize for Img {
        fn cache_size(&self) -> usize {
            self.data.len()
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct ImgMeta {
        w: u16,
        h: u16,
    }

    impl HasSize for ImgMeta {
        fn cache_size(&self) -> usize {
            1
        }
    }

    impl HasProbe for Img {
        type Meta = ImgMeta;
        fn extract_meta(&self) -> ImgMeta {
            ImgMeta {
                w: self.w,
                h: self.h,
            }
        }
    }

    #[test]
    fn closure_loader_returns_ok() {
        let loader = |token: &str| -> Result<Img, LoadError> {
            if token == "logo" {
                Ok(Img {
                    data: "bytes".to_string(),
                    w: 16,
                    h: 16,
                })
            } else {
                Err(LoadError::NotMine)
            }
        };
        let mut cx = LoadContext::new("logo", core::task::Waker::noop(), 4);
        assert_eq!(
            loader
                .poll_load(&mut (), &mut cx)
                .map(|result| result.map(|i| (i.w, i.h))),
            Poll::Ready(Ok((16, 16)))
        );
        let mut cx = LoadContext::new("other", core::task::Waker::noop(), 4);
        assert_eq!(
            loader.poll_load(&mut (), &mut cx),
            Poll::Ready(Err(LoadError::NotMine))
        );
    }

    #[test]
    fn explicit_probe_avoids_full_load() {
        struct ProbeAware;
        impl Loader<Img> for ProbeAware {
            type State = ();
            fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
                FormatMatch::Match
            }
            fn poll_load(
                &self,
                _: &mut (),
                _: &mut LoadContext<'_>,
            ) -> Poll<Result<Img, LoadError>> {
                panic!("metadata must not decode pixels");
            }
        }
        impl ProbeLoader<Img> for ProbeAware {
            fn poll_probe(
                &self,
                _: &mut (),
                _: &mut LoadContext<'_>,
            ) -> Poll<Result<ImgMeta, LoadError>> {
                Poll::Ready(Ok(ImgMeta { w: 64, h: 32 }))
            }
        }
        let manager = super::super::ResourceManager::new(
            crate::core::cache::MaxSize::Count(4),
            Img {
                data: String::new(),
                w: 1,
                h: 1,
            },
        );
        manager.add_probed_loader(ProbeAware);
        assert_eq!(manager.probe("hero"), Some(ImgMeta { w: 64, h: 32 }));
    }
}
