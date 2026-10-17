use mirui::prelude::*;
use mirui::ui::Style;
use mirui::ui::widgets::navigation::{self, CreatePage, NavigationPath, NavigationStack};
use mirui::ui::widgets::page_view::{self, CreatePage as CreateSelectedPage, PageView};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[model]
struct Document {
    #[observe]
    width: u16,
    #[observe]
    page: u8,
}

#[model]
impl Document {
    fn resize(&mut self, width: u16) {
        self.width = width;
    }

    fn select(&mut self, page: u8) {
        self.page = page;
    }
}

#[derive(Clone)]
struct Route;

#[derive(Default)]
struct PageProbe {
    reads: Cell<usize>,
    taps: Cell<usize>,
}

impl PageProbe {
    fn width(&self, width: u16) -> u16 {
        self.reads.set(self.reads.get() + 1);
        width
    }
}

#[derive(Default)]
struct Pages(RefCell<Vec<(Entity, Rc<PageProbe>)>>);

#[compose]
fn content(document: model!(Document)) -> Entity {
    let probe = Rc::new(PageProbe::default());
    let reads = probe.clone();
    let taps = probe.clone();
    let node = ui! {
        View (width: ${ reads.width(document.width()) }, height: 20) on Tap { taps.taps.set(taps.taps.get() + 1); }
    };
    res!(Pages).with(|pages| pages.0.borrow_mut().push((node, probe)));
    node
}

#[event]
fn create_page(_event: event!(CreatePage<Route>), document: model!(Document)) -> Entity {
    ui! {
        content (document)
    }
}

#[compose]
fn stack(path: model!(NavigationPath<Route>), document: model!(Document)) -> Entity {
    ui! {
        NavigationStack (path: path, grow: 1.0) {
            content (document)
        } on CreatePage(create_page::callback(document))
    }
}

#[test]
fn retained_pages_pause_observation_and_release_callbacks_when_retired() {
    let mut app = App::headless(80, 80);
    app.with_default_widgets();
    app.with_widget(navigation::view::<Route>());
    app.add_resource(Pages::default());
    let root = app.spawn_root().id();
    let document = app.add_model(Document { width: 20, page: 0 }).finish();
    let path = app
        .add_model(NavigationPath::<Route>::with_capacity(2))
        .finish();
    let removals = app.add_model(super::State::default()).finish();
    app.add_system(super::delete_pending::system(removals.clone()));
    let container = app.compose(root, stack(path.clone(), document.clone()));
    app.render().unwrap();
    let (home, home_probe) = app.world.resource::<Pages>().unwrap().0.borrow()[0].clone();

    path.push(Route).unwrap();
    app.render().unwrap();
    let (detail, detail_probe) = app.world.resource::<Pages>().unwrap().0.borrow()[1].clone();
    let hidden_reads = home_probe.reads.get();
    let visible_reads = detail_probe.reads.get();
    document.resize(35);
    app.render().unwrap();
    assert_eq!(home_probe.reads.get(), hidden_reads);
    assert!(detail_probe.reads.get() > visible_reads);
    assert_eq!(
        app.world.get::<Style>(home).unwrap().layout.width,
        Dimension::px(20)
    );
    assert_eq!(
        app.world.get::<Style>(detail).unwrap().layout.width,
        Dimension::px(35)
    );

    path.pop().unwrap();
    app.render().unwrap();
    assert!(home_probe.reads.get() > hidden_reads);
    assert_eq!(
        app.world.get::<Style>(home).unwrap().layout.width,
        Dimension::px(35)
    );
    assert!(!app.world.is_alive(detail));
    assert_eq!(Rc::strong_count(&detail_probe), 2);
    let retired_reads = detail_probe.reads.get();

    removals.remove(container);
    app.tick();
    assert!(!app.world.is_alive(home));
    assert_eq!(Rc::strong_count(&home_probe), 2);
    let retired_home_reads = home_probe.reads.get();
    document.resize(40);
    app.render().unwrap();
    assert_eq!(home_probe.reads.get(), retired_home_reads);
    assert_eq!(detail_probe.reads.get(), retired_reads);
    assert_eq!(document.width(), 40);

    app.compose(root, stack(path.clone(), document));
    app.render().unwrap();
    path.push(Route).unwrap();
    app.render().unwrap();
    assert_eq!(app.world.resource::<Pages>().unwrap().0.borrow().len(), 4);
}

#[event]
fn create_selected_page(
    _event: event!(CreateSelectedPage<u8>),
    document: model!(Document),
) -> Entity {
    ui! {
        content (document)
    }
}

#[compose]
fn selected_page(document: model!(Document), transition: navigation::PageTransition) -> Entity {
    ui! {
        PageView (page: ${ document.page() }, transition: transition, grow: 1.0) on CreatePage(create_selected_page::callback(document))
    }
}

#[test]
fn removing_an_animated_container_releases_both_pages_and_the_selection_binding() {
    for transition in [
        mirui::anim::Transition::from(mirui::anim::BOUNCY),
        mirui::anim::TweenConfig::new(300, &mirui::anim::ease::OUT_CUBIC).into(),
    ] {
        let mut app = App::headless(80, 80);
        app.with_default_widgets();
        app.with_widget(page_view::view::<u8>());
        app.add_resource(Pages::default());
        let root = app.spawn_root().id();
        let document = app.add_model(Document { width: 20, page: 0 }).finish();
        let removals = app.add_model(super::State::default()).finish();
        app.add_system(super::delete_pending::system(removals.clone()));
        let container = app.compose(
            root,
            selected_page(
                document.clone(),
                navigation::PageTransition::slide(transition),
            ),
        );
        app.render().unwrap();
        document.select(1);
        app.render().unwrap();
        app.tick();
        app.render().unwrap();
        let pages = app.world.resource::<Pages>().unwrap().0.borrow().clone();
        assert_eq!(pages.len(), 2);
        for (node, probe) in &pages {
            assert!(app.world.is_alive(*node));
            assert!(Rc::strong_count(probe) > 2);
        }

        removals.remove(container);
        app.tick();
        for (node, probe) in &pages {
            assert!(!app.world.is_alive(*node));
            assert_eq!(Rc::strong_count(probe), 2);
        }
        let reads: Vec<_> = pages.iter().map(|(_, probe)| probe.reads.get()).collect();
        document.resize(40);
        document.select(2);
        for _ in 0..3 {
            app.tick();
            app.render().unwrap();
        }
        assert_eq!(app.world.resource::<Pages>().unwrap().0.borrow().len(), 2);
        for ((_, probe), reads) in pages.iter().zip(reads) {
            assert_eq!(probe.reads.get(), reads);
        }
    }
}
