//! Generic resource manager. Token-string identification, [`core::cell::RefCell`]
//! interior mutability for [`&World`]-only access from render code, and an
//! optional probe sidecar gated on [`HasProbe`].
//!
//! See `.local/specs/1.0-resource-management/design.md` for the full design.

extern crate alloc;

mod handle;
mod loader;
mod manager;
mod manager_inner;
mod probe;

pub use handle::ResourceHandle;
pub use loader::{LoadError, Loader, ProbeLoader};
pub use manager::ResourceManager;
pub use probe::HasProbe;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cache::{HasSize, MaxSize};
    use alloc::rc::Rc;
    use alloc::string::{String, ToString};

    #[derive(Clone, Debug, PartialEq)]
    struct Note(String);

    impl HasSize for Note {
        fn cache_size(&self) -> usize {
            self.0.len().max(1)
        }
    }

    fn fb_note() -> Note {
        Note("FALLBACK".into())
    }

    #[derive(Clone, Debug, PartialEq)]
    struct Img {
        bytes: String,
        w: u16,
        h: u16,
    }

    #[derive(Clone, Debug, PartialEq)]
    struct ImgMeta {
        w: u16,
        h: u16,
    }

    impl HasSize for Img {
        fn cache_size(&self) -> usize {
            self.bytes.len().max(1)
        }
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

    fn fb_img() -> Img {
        Img {
            bytes: "FALLBACK".into(),
            w: 16,
            h: 16,
        }
    }

    fn fb_meta() -> ImgMeta {
        ImgMeta { w: 16, h: 16 }
    }

    #[test]
    fn add_static_then_resolve_returns_value() {
        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        m.add_static("greeting", Note("hi".into()));
        let rc = m.resolve("greeting");
        assert_eq!(*rc, Note("hi".into()));
    }

    #[test]
    fn unknown_token_returns_fallback() {
        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        let rc = m.resolve("nonexistent");
        assert_eq!(*rc, fb_note());
    }

    #[test]
    fn add_factory_runs_lazily_once() {
        use core::cell::Cell;

        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        let runs = Rc::new(Cell::new(0u32));
        let runs2 = runs.clone();
        m.add_factory("page", move || {
            runs2.set(runs2.get() + 1);
            Some(Note("page-content".into()))
        });
        assert_eq!(runs.get(), 0, "factory should not run before resolve");

        let a = m.resolve("page");
        assert_eq!(*a, Note("page-content".into()));
        assert_eq!(runs.get(), 1);

        let b = m.resolve("page");
        assert_eq!(*b, Note("page-content".into()));
        assert_eq!(
            runs.get(),
            1,
            "second resolve must hit cache, not re-run factory"
        );
    }

    #[test]
    fn closure_loader_chain_first_match_wins() {
        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        m.add_loader(|t: &str| -> Result<Note, LoadError> {
            if t.starts_with("first/") {
                Ok(Note(format!("from-first:{}", t).to_string()))
            } else {
                Err(LoadError::NotMine)
            }
        });
        m.add_loader(|t: &str| -> Result<Note, LoadError> {
            if t.starts_with("second/") {
                Ok(Note(format!("from-second:{}", t).to_string()))
            } else {
                Err(LoadError::NotMine)
            }
        });

        assert_eq!(*m.resolve("first/x"), Note("from-first:first/x".into()));
        assert_eq!(*m.resolve("second/y"), Note("from-second:second/y".into()));
        assert_eq!(*m.resolve("unhandled"), fb_note());
    }

    #[test]
    fn loader_failed_short_circuits_chain_and_marks_failed() {
        use core::cell::Cell;

        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        let later_calls = Rc::new(Cell::new(0u32));
        let later2 = later_calls.clone();
        m.add_loader(|_t: &str| -> Result<Note, LoadError> { Err(LoadError::Failed("bad token")) });
        m.add_loader(move |_t: &str| -> Result<Note, LoadError> {
            later2.set(later2.get() + 1);
            Ok(Note("late".into()))
        });

        let r = m.resolve("anything");
        assert_eq!(
            *r,
            fb_note(),
            "Failed should short-circuit and return fallback"
        );
        assert_eq!(
            later_calls.get(),
            0,
            "later loaders should not run after Failed"
        );

        let _ = m.resolve("anything");
        assert_eq!(
            later_calls.get(),
            0,
            "failed-set must prevent re-running loaders on repeat resolve",
        );
    }

    #[test]
    fn handle_keeps_token_alive_via_load() {
        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        m.add_static("greeting", Note("hi".into()));

        let h = m.load("greeting");
        let v: Rc<Note> = h.get();
        assert_eq!(*v, Note("hi".into()));
        assert_eq!(h.token(), "greeting");
    }

    #[test]
    fn handle_get_after_manager_dropped_returns_fallback() {
        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        m.add_static("greeting", Note("hi".into()));
        let h = m.load("greeting");
        drop(m);
        let v = h.get();
        assert_eq!(*v, fb_note());
    }

    #[test]
    fn remove_token_drops_factory_but_existing_rcs_survive() {
        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        m.add_static("a", Note("alpha".into()));

        let still_held = m.resolve("a");
        m.remove_token("a");

        assert_eq!(
            *still_held,
            Note("alpha".into()),
            "existing Rc<T> survives unregistration",
        );

        let after = m.resolve("a");
        assert_eq!(*after, fb_note(), "new resolve falls back after unregister");
    }

    #[test]
    fn subscribe_returns_signal_for_token() {
        let m = ResourceManager::<Note>::new(MaxSize::Bytes(1024), fb_note());
        let s1 = m.subscribe("foo");
        let s2 = m.subscribe("foo");
        assert!(
            Rc::ptr_eq(&s1, &s2),
            "subscribe is idempotent for the same token"
        );
    }

    fn img_manager() -> ResourceManager<Img> {
        let m = ResourceManager::<Img>::new(MaxSize::Bytes(1024), fb_img());
        m.enable_probes(MaxSize::Count(64), fb_meta());
        m
    }

    #[test]
    fn probe_via_static_extracts_meta_without_full_decode() {
        let m = img_manager();
        m.add_static(
            "hero",
            Img {
                bytes: "long-decoded-pixel-data".into(),
                w: 128,
                h: 64,
            },
        );

        let meta = m.probe("hero");
        assert_eq!(meta, Some(ImgMeta { w: 128, h: 64 }));
    }

    #[test]
    fn probe_via_factory_returns_none_without_probe_path() {
        let m = img_manager();
        m.add_factory("dyn", || {
            Some(Img {
                bytes: "ran".into(),
                w: 32,
                h: 32,
            })
        });
        assert_eq!(
            m.probe("dyn"),
            None,
            "plain Factory cannot answer probe without running the factory",
        );
    }

    #[test]
    fn add_probed_factory_answers_probe_without_running_factory() {
        use core::cell::Cell;
        let m = img_manager();
        let runs = Rc::new(Cell::new(0u32));
        let runs2 = runs.clone();
        m.add_probed_factory("logo", ImgMeta { w: 256, h: 128 }, move || {
            runs2.set(runs2.get() + 1);
            Some(Img {
                bytes: "decoded".into(),
                w: 256,
                h: 128,
            })
        });

        assert_eq!(m.probe("logo"), Some(ImgMeta { w: 256, h: 128 }));
        assert_eq!(runs.get(), 0, "probe should not run the factory");

        let v = m.resolve("logo");
        assert_eq!(v.w, 256);
        assert_eq!(runs.get(), 1, "resolve runs factory once");
    }

    #[test]
    fn resolve_mirrors_meta_into_probes_cache() {
        let m = img_manager();
        m.add_static(
            "hero",
            Img {
                bytes: "data".into(),
                w: 100,
                h: 50,
            },
        );
        let _ = m.resolve("hero");
        let meta = m.probe("hero");
        assert_eq!(meta, Some(ImgMeta { w: 100, h: 50 }));
    }

    #[test]
    fn probe_loader_chain_returns_meta_only() {
        let m = img_manager();
        m.add_loader(|t: &str| -> Result<Img, LoadError> {
            if let Some(rest) = t.strip_prefix("img/") {
                Ok(Img {
                    bytes: format!("decoded:{}", rest).to_string(),
                    w: 24,
                    h: 24,
                })
            } else {
                Err(LoadError::NotMine)
            }
        });

        let meta = m.probe("img/foo");
        assert_eq!(meta, Some(ImgMeta { w: 24, h: 24 }));
    }

    #[test]
    fn unknown_token_probe_returns_none() {
        let m = img_manager();
        assert_eq!(m.probe("nope"), None);
    }

    #[test]
    fn builder_with_chain_initializes_full_manager() {
        let m = ResourceManager::<Img>::new(MaxSize::Bytes(1024), fb_img())
            .with_probes(MaxSize::Count(64), fb_meta())
            .with_static(
                "logo",
                Img {
                    bytes: "static-logo".into(),
                    w: 64,
                    h: 64,
                },
            )
            .with_probed_factory("hero", ImgMeta { w: 256, h: 128 }, || {
                Some(Img {
                    bytes: "hero-decoded".into(),
                    w: 256,
                    h: 128,
                })
            })
            .with_loader(|t: &str| -> Result<Img, LoadError> {
                if t == "fallback-route" {
                    Ok(Img {
                        bytes: "loader-out".into(),
                        w: 8,
                        h: 8,
                    })
                } else {
                    Err(LoadError::NotMine)
                }
            });

        assert_eq!(m.probe("logo"), Some(ImgMeta { w: 64, h: 64 }));
        assert_eq!(m.resolve("logo").w, 64);

        assert_eq!(m.probe("hero"), Some(ImgMeta { w: 256, h: 128 }));

        assert_eq!(m.resolve("fallback-route").w, 8);
    }
}
