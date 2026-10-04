use mirui::__macro_support::NotificationQueue;
use mirui::types::Fixed;
use mirui::ui::widgets::slider;

fn main() {
    let queue = NotificationQueue::default();
    let mut retained = None;
    let _ = queue.flush(1, |notification| {
        retained = notification.payload::<slider::ValueChanged>();
        Ok::<(), core::convert::Infallible>(())
    });
    let _ = retained;
    let _ = queue.flush(1, |notification| {
        notification.payload::<slider::ValueChanged>().unwrap().new = Fixed::ZERO;
        Ok::<(), core::convert::Infallible>(())
    });
}
