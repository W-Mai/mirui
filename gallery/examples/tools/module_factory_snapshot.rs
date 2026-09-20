mod lab_snapshot_support;

use mirui::gallery::demos::module_factory;

fn main() {
    lab_snapshot_support::run(
        "module-factory.png",
        module_factory::VIEWPORT,
        |app, root| {
            module_factory::setup_app(app, root);
        },
    );
}
