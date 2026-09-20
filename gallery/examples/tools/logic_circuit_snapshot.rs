mod lab_snapshot_support;

use mirui::gallery::demos::logic_circuit;

fn main() {
    lab_snapshot_support::run("logic-circuit.png", logic_circuit::VIEWPORT, |app, root| {
        logic_circuit::setup_app(app, root);
    });
}
