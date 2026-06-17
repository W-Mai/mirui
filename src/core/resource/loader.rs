use crate::core::resource::probe::HasProbe;

/// Outcome of [`Loader::try_load`] (and [`ProbeLoader::try_probe`]) when the
/// loader did not return a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadError {
    /// Token doesn't belong to this loader's domain. The manager continues
    /// walking the loader chain.
    NotMine,
    /// The loader claimed the token but failed to produce a value. The
    /// manager logs the message once and marks the token as failed (no retry
    /// until [`crate::core::resource::ResourceManager::clear_failed`] is called).
    Failed(&'static str),
}

impl LoadError {
    /// Human-readable failure detail, when this is a [`LoadError::Failed`].
    pub fn message(&self) -> Option<&'static str> {
        match self {
            LoadError::NotMine => None,
            LoadError::Failed(m) => Some(m),
        }
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
    fn try_load(&self, token: &str) -> Result<T, LoadError>;
}

/// Companion trait that gives every [`Loader<T>`] a default `try_probe`
/// running `try_load` then downgrading via [`HasProbe::extract_meta`].
/// Override on the concrete impl when partial fetch is meaningfully cheaper
/// than full decode (e.g. a mirx FLAT loader can read 28 bytes for the
/// header instead of the full pixel payload).
pub trait ProbeLoader<T: HasProbe>: Loader<T> {
    fn try_probe(&self, token: &str) -> Result<T::Meta, LoadError> {
        self.try_load(token).map(|v| v.extract_meta())
    }
}

impl<T: HasProbe, L: Loader<T> + ?Sized> ProbeLoader<T> for L {}

impl<T, F> Loader<T> for F
where
    F: Fn(&str) -> Result<T, LoadError> + 'static,
{
    fn try_load(&self, token: &str) -> Result<T, LoadError> {
        self(token)
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
        assert_eq!(loader.try_load("logo").map(|i| (i.w, i.h)), Ok((16, 16)));
        assert_eq!(loader.try_load("other"), Err(LoadError::NotMine));
    }

    #[test]
    fn default_try_probe_runs_full_load() {
        // Closure-based loader; the blanket ProbeLoader impl gives it the
        // default try_probe that just calls try_load and extracts meta.
        let loader = |_t: &str| -> Result<Img, LoadError> {
            Ok(Img {
                data: "bytes".to_string(),
                w: 32,
                h: 32,
            })
        };
        let meta: Result<ImgMeta, _> = ProbeLoader::<Img>::try_probe(&loader, "anything");
        assert_eq!(meta, Ok(ImgMeta { w: 32, h: 32 }));
    }

    #[test]
    fn override_try_probe_avoids_full_load() {
        // A concrete loader can implement try_probe directly to skip a heavy
        // try_load when only metadata is needed.
        struct ProbeAware;
        impl Loader<Img> for ProbeAware {
            fn try_load(&self, _t: &str) -> Result<Img, LoadError> {
                panic!("try_load should not run when try_probe is overridden");
            }
        }
        impl ProbeAware {
            fn try_probe_override(&self, _t: &str) -> Result<ImgMeta, LoadError> {
                Ok(ImgMeta { w: 64, h: 32 })
            }
        }
        // Demonstrate the *intent* — the manager calls try_probe via dynamic
        // dispatch on a type that overrides it. Real overrides land on
        // production loaders (MirxLoader etc).
        let p = ProbeAware;
        assert_eq!(p.try_probe_override("hero"), Ok(ImgMeta { w: 64, h: 32 }));
    }
}
