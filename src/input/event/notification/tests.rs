use super::*;
use alloc::rc::Rc;
use core::cell::Cell;

use crate::input::event::contract::{AppSource, WidgetSource};
use crate::test_support::tracked_allocations;

fn flush(
    queue: &NotificationQueue,
    limit: usize,
    mut run: impl FnMut(&Notification<'_>),
) -> Result<usize, NotificationError> {
    queue
        .flush(limit, |notification| {
            run(notification);
            Ok::<(), core::convert::Infallible>(())
        })
        .map_err(|error| match error {
            NotificationFlushError::Storage(error) => error,
            NotificationFlushError::Listener(never) => match never {},
        })
}

#[test]
fn listener_error_preserves_the_error_and_drops_unread_and_generated_payloads() {
    struct Failure(Rc<Cell<usize>>);
    impl Drop for Failure {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let queue = NotificationQueue::default();
    queue
        .reserve(capacity(
            3,
            3 * size_of::<Owned>(),
            core::mem::align_of::<Owned>(),
        ))
        .unwrap();
    let drops = Rc::new(Cell::new(0));
    let errors = Rc::new(Cell::new(0));
    for value in 0..3 {
        queue
            .push_app(Owned {
                value,
                drops: drops.clone(),
            })
            .unwrap();
    }
    let allocations = tracked_allocations(|| {
        let error = queue
            .flush(4, |notification| {
                let value = notification.payload::<Owned>().unwrap();
                assert_eq!(value.value, 0);
                queue
                    .push_app(Owned {
                        value: 9,
                        drops: drops.clone(),
                    })
                    .unwrap();
                Err(Failure(errors.clone()))
            })
            .err()
            .unwrap();
        assert!(matches!(error, NotificationFlushError::Listener(_)));
        assert_eq!(drops.get(), 4);
        assert_eq!(errors.get(), 0);
        drop(error);
        assert_eq!(errors.get(), 1);
        queue.push_app(Empty).unwrap();
        assert_eq!(queue.flush(1, |_| Ok::<(), Failure>(())).ok(), Some(1));
    });
    assert_eq!(allocations, 0);
}

struct Widget;
impl crate::ecs::Component for Widget {}

#[derive(Debug)]
struct Value(u32);

impl Event for Value {
    type Route = EntityNotification;
    type Source = WidgetSource<Widget>;
    type Options = ();
}

#[derive(Debug)]
struct Empty;

impl Event for Empty {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

#[derive(Debug)]
#[repr(align(128))]
struct Aligned([u8; 128]);

impl Event for Aligned {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

#[derive(Debug)]
struct Owned {
    value: u32,
    drops: Rc<Cell<usize>>,
}

impl Event for Owned {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

impl Drop for Owned {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

fn entity() -> Entity {
    Entity {
        id: 3,
        generation: 7,
    }
}

fn capacity(
    records: usize,
    payload_bytes: usize,
    payload_alignment: usize,
) -> NotificationCapacity {
    NotificationCapacity {
        records,
        payload_bytes,
        payload_alignment,
    }
}

#[test]
fn business_quotas_and_single_app_records_ignore_physical_slack_and_reuse_after_cleanup() {
    use crate::input::event::lifecycle::{Resumed, Started};

    let queue = NotificationQueue::default();
    queue.reserve(capacity(5, 20, 4)).unwrap();
    let business = capacity(1, 4, 4);
    for _ in 0..2 {
        assert_eq!(
            tracked_allocations(|| {
                queue.push_business(entity(), Value(1), business).unwrap();
                queue.push_app_notice(Started).unwrap();
                assert!(matches!(
                    queue.push_business(entity(), Value(2), business),
                    Err((NotificationError::RecordCapacity { available: 1 }, Value(2)))
                ));
                assert!(matches!(
                    queue.push_app_notice(Resumed),
                    Err((NotificationError::RecordCapacity { available: 1 }, Resumed))
                ));
                assert_eq!(
                    flush(&queue, 2, |_| {
                        assert!(matches!(
                            queue.push_app_notice(Resumed),
                            Err((NotificationError::RecordCapacity { available: 1 }, Resumed))
                        ));
                    }),
                    Ok(2)
                );
            }),
            0
        );
    }
    queue.push_app_notice(Started).unwrap();
    assert_eq!(
        flush(&queue, 0, |_| {}),
        Err(NotificationError::FlushLimit { limit: 0 })
    );
    queue.push_app_notice(Resumed).unwrap();
    queue.clear().unwrap();
    queue.push_business(entity(), Value(3), business).unwrap();
    queue.push_app_notice(Started).unwrap();
    assert_eq!(flush(&queue, 2, |_| {}), Ok(2));
}

#[test]
fn double_buffer_preserves_fifo_and_payload_window_without_allocating() {
    let queue = NotificationQueue::default();
    queue.reserve(capacity(4, 16, 4)).unwrap();
    let mut observed = [0; 4];
    let mut count = 0;
    let allocations = tracked_allocations(|| {
        queue.push_entity(entity(), Value(1)).unwrap();
        queue.push_entity(entity(), Value(2)).unwrap();
        queue.push_entity(entity(), Value(3)).unwrap();
        let dispatched = flush(&queue, 4, |notification| {
            assert_eq!(notification.entity(), Some(entity()));
            assert!(notification.payload::<Empty>().is_none());
            let value = notification.payload::<Value>().unwrap();
            let pointer = value as *const Value;
            observed[count] = value.0;
            count += 1;
            if value.0 == 1 {
                queue.push_entity(entity(), Value(4)).unwrap();
                assert_eq!(
                    notification.payload::<Value>().unwrap() as *const Value,
                    pointer
                );
                assert_eq!(value.0, 1);
            }
        })
        .unwrap();
        assert_eq!(dispatched, 4);
    });
    assert_eq!(allocations, 0);
    assert_eq!(observed, [1, 2, 3, 4]);
    assert_eq!(flush(&queue, 0, |_| unreachable!()), Ok(0));
}

#[test]
fn record_payload_and_alignment_limits_preserve_rejected_ownership() {
    let queue = NotificationQueue::default();
    queue.reserve(capacity(1, 1, 1)).unwrap();
    let (error, value) = queue.push_entity(entity(), Value(11)).unwrap_err();
    assert_eq!(
        error,
        NotificationError::PayloadAlignment {
            required: 4,
            available: 1
        }
    );
    assert_eq!(value.0, 11);
    queue.reserve(capacity(1, 1, 4)).unwrap();
    let (error, value) = queue.push_entity(entity(), value).unwrap_err();
    assert_eq!(
        error,
        NotificationError::PayloadCapacity {
            required: 4,
            available: 1
        }
    );
    queue.reserve(capacity(1, 4, 4)).unwrap();
    queue.push_entity(entity(), value).unwrap();
    let (error, _) = queue.push_app(Empty).unwrap_err();
    assert_eq!(error, NotificationError::RecordCapacity { available: 1 });
    assert_eq!(
        flush(&queue, 1, |n| assert_eq!(
            n.payload::<Value>().unwrap().0,
            11
        )),
        Ok(1)
    );
    let drops = Rc::new(Cell::new(0));
    let (error, rejected) = queue
        .push_app(Owned {
            value: 0,
            drops: drops.clone(),
        })
        .unwrap_err();
    assert!(matches!(
        error,
        NotificationError::PayloadAlignment { .. } | NotificationError::PayloadCapacity { .. }
    ));
    assert_eq!(drops.get(), 0);
    drop(rejected);
    assert_eq!(drops.get(), 1);
}

#[test]
fn aligned_payloads_and_zero_byte_notifications_round_trip() {
    let queue = NotificationQueue::default();
    queue.reserve(capacity(2, 0, 1)).unwrap();
    queue.push_app(Empty).unwrap();
    assert_eq!(
        flush(&queue, 1, |n| {
            assert_eq!(n.entity(), None);
            assert!(n.payload::<Empty>().is_some());
        }),
        Ok(1)
    );
    queue.reserve(capacity(2, 256, 128)).unwrap();
    queue.push_entity(entity(), Value(7)).unwrap();
    queue.push_app(Aligned([9; 128])).unwrap();
    assert_eq!(
        flush(&queue, 2, |n| {
            if let Some(value) = n.payload::<Aligned>() {
                assert_eq!(value as *const Aligned as usize % 128, 0);
                assert_eq!(value.0, [9; 128]);
            }
        }),
        Ok(2)
    );
}

#[test]
fn callback_reentry_and_reconfiguration_are_explicitly_rejected() {
    let queue = NotificationQueue::default();
    queue.reserve(capacity(2, 0, 1)).unwrap();
    queue.push_app(Empty).unwrap();
    flush(&queue, 1, |_| {
        assert_eq!(
            flush(&queue, 1, |_| unreachable!()),
            Err(NotificationError::Busy)
        );
        assert_eq!(queue.clear(), Err(NotificationError::Busy));
        assert_eq!(
            queue.reserve(capacity(3, 8, 8)),
            Err(NotificationError::Busy)
        );
    })
    .unwrap();
    assert_eq!(queue.reserve(capacity(3, 8, 8)), Ok(()));
}

#[test]
fn feedback_limit_drops_remaining_and_generated_payloads_once_and_reuses_storage() {
    let drops = Rc::new(Cell::new(0));
    let queue = NotificationQueue::default();
    queue
        .reserve(capacity(
            3,
            3 * size_of::<Owned>(),
            core::mem::align_of::<Owned>(),
        ))
        .unwrap();
    queue
        .push_app(Owned {
            value: 0,
            drops: drops.clone(),
        })
        .unwrap();
    queue
        .push_app(Owned {
            value: 1,
            drops: drops.clone(),
        })
        .unwrap();
    let allocations = tracked_allocations(|| {
        let result = flush(&queue, 3, |n| {
            let value = n.payload::<Owned>().unwrap();
            queue
                .push_app(Owned {
                    value: value.value + 2,
                    drops: drops.clone(),
                })
                .unwrap();
        });
        assert_eq!(result, Err(NotificationError::FlushLimit { limit: 3 }));
    });
    assert_eq!(allocations, 0);
    assert_eq!(drops.get(), 5);
    assert_eq!(flush(&queue, 0, |_| unreachable!()), Ok(0));
    let allocations = tracked_allocations(|| {
        queue
            .push_app(Owned {
                value: 7,
                drops: drops.clone(),
            })
            .unwrap();
        assert_eq!(
            flush(&queue, 1, |n| assert_eq!(
                n.payload::<Owned>().unwrap().value,
                7
            )),
            Ok(1)
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(drops.get(), 6);
}

#[test]
fn callback_unwind_drops_both_buffers_and_resets_the_runtime() {
    let drops = Rc::new(Cell::new(0));
    let queue = NotificationQueue::default();
    queue
        .reserve(capacity(
            2,
            2 * size_of::<Owned>(),
            core::mem::align_of::<Owned>(),
        ))
        .unwrap();
    queue
        .push_app(Owned {
            value: 1,
            drops: drops.clone(),
        })
        .unwrap();
    queue
        .push_app(Owned {
            value: 2,
            drops: drops.clone(),
        })
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = flush(&queue, 2, |_| {
            queue
                .push_app(Owned {
                    value: 3,
                    drops: drops.clone(),
                })
                .unwrap();
            panic!("notification callback failed");
        });
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 3);
    let allocations = tracked_allocations(|| {
        queue.push_app(Empty).unwrap();
        assert_eq!(flush(&queue, 1, |_| ()), Ok(1));
    });
    assert_eq!(allocations, 0);
}

#[test]
fn failed_reservation_keeps_pending_values_and_logical_limits() {
    let queue = NotificationQueue::default();
    queue.reserve(capacity(1, 4, 4)).unwrap();
    queue.push_entity(entity(), Value(1)).unwrap();
    assert_eq!(
        queue.reserve(capacity(2, 8, 3)),
        Err(NotificationError::InvalidLayout)
    );
    assert_eq!(
        queue.reserve(capacity(usize::MAX, 8, 8)),
        Err(NotificationError::InvalidLayout)
    );
    let (error, _) = queue.push_entity(entity(), Value(2)).unwrap_err();
    assert_eq!(error, NotificationError::RecordCapacity { available: 1 });
    flush(&queue, 1, |n| {
        assert_eq!(n.payload::<Value>().unwrap().0, 1)
    })
    .unwrap();
}

#[test]
fn high_alignment_zero_sized_drop_records_need_no_payload_bytes() {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DROPS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug)]
    #[repr(align(128))]
    struct EmptyDrop;

    impl Event for EmptyDrop {
        type Route = AppNotification;
        type Source = AppSource;
        type Options = ();
    }

    impl Drop for EmptyDrop {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }

    let queue = NotificationQueue::default();
    queue.reserve(capacity(2, 0, 1)).unwrap();
    let allocations = tracked_allocations(|| {
        queue.push_app(EmptyDrop).unwrap();
        queue.push_app(EmptyDrop).unwrap();
        assert_eq!(
            flush(&queue, 1, |n| assert!(n.payload::<EmptyDrop>().is_some())),
            Err(NotificationError::FlushLimit { limit: 1 })
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(DROPS.load(Ordering::Relaxed), 2);
}

#[test]
fn clear_and_queue_drop_destroy_owned_payloads_once() {
    let drops = Rc::new(Cell::new(0));
    {
        let queue = NotificationQueue::default();
        queue
            .reserve(capacity(
                2,
                2 * size_of::<Owned>(),
                core::mem::align_of::<Owned>(),
            ))
            .unwrap();
        queue
            .push_app(Owned {
                value: 1,
                drops: drops.clone(),
            })
            .unwrap();
        queue.clear().unwrap();
        assert_eq!(drops.get(), 1);
        queue
            .push_app(Owned {
                value: 2,
                drops: drops.clone(),
            })
            .unwrap();
        queue
            .push_app(Owned {
                value: 3,
                drops: drops.clone(),
            })
            .unwrap();
    }
    assert_eq!(drops.get(), 3);
}

#[test]
fn payload_destructor_cannot_reenter_storage_cleanup() {
    struct OnDrop {
        queue: Rc<NotificationQueue>,
        rejected: Rc<Cell<bool>>,
    }
    impl Event for OnDrop {
        type Route = AppNotification;
        type Source = AppSource;
        type Options = ();
    }
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.rejected.set(matches!(
                self.queue.push_app(Empty),
                Err((NotificationError::Busy, _))
            ));
        }
    }
    let queue = Rc::new(NotificationQueue::default());
    let rejected = Rc::new(Cell::new(false));
    queue
        .reserve(capacity(
            2,
            size_of::<OnDrop>(),
            core::mem::align_of::<OnDrop>(),
        ))
        .unwrap();
    queue
        .push_app(OnDrop {
            queue: queue.clone(),
            rejected: rejected.clone(),
        })
        .unwrap_or_else(|_| panic!("reserved queue"));
    queue.clear().unwrap();
    assert!(rejected.get());
    rejected.set(false);
    queue
        .push_app(OnDrop {
            queue: queue.clone(),
            rejected: rejected.clone(),
        })
        .unwrap_or_else(|_| panic!("reserved queue"));
    assert_eq!(
        flush(&queue, 0, |_| unreachable!()),
        Err(NotificationError::FlushLimit { limit: 0 })
    );
    assert!(rejected.get());
}

#[test]
fn bounded_capacity_survives_repeated_swaps_and_smaller_reservation_requests() {
    let queue = NotificationQueue::default();
    queue.reserve(capacity(2, 8, 4)).unwrap();
    queue.reserve(capacity(1, 4, 1)).unwrap();
    let allocations = tracked_allocations(|| {
        for _ in 0..32 {
            queue.push_entity(entity(), Value(0)).unwrap();
            queue.push_entity(entity(), Value(1)).unwrap();
            let mut observed = [0; 4];
            let mut count = 0;
            assert_eq!(
                flush(&queue, 4, |n| {
                    let value = n.payload::<Value>().unwrap().0;
                    observed[count] = value;
                    count += 1;
                    if value == 0 {
                        queue.push_entity(entity(), Value(2)).unwrap();
                        queue.push_entity(entity(), Value(3)).unwrap();
                        assert!(matches!(
                            queue.push_entity(entity(), Value(4)),
                            Err((NotificationError::RecordCapacity { available: 2 }, Value(4)))
                        ));
                    }
                }),
                Ok(4)
            );
            assert_eq!(observed, [0, 1, 2, 3]);
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn unreserved_queue_is_empty_and_never_allocates_implicitly() {
    let allocations = tracked_allocations(|| {
        let queue = NotificationQueue::default();
        assert!(matches!(
            queue.push_app(Empty),
            Err((NotificationError::RecordCapacity { available: 0 }, Empty))
        ));
        assert_eq!(flush(&queue, 0, |_| unreachable!()), Ok(0));
        queue.clear().unwrap();
    });
    assert_eq!(allocations, 0);
}

#[test]
fn payload_destructor_unwind_cleans_remaining_records_and_restores_buffers() {
    #[derive(Debug)]
    struct PanicOnce {
        panic: Rc<Cell<bool>>,
        drops: Rc<Cell<usize>>,
    }
    impl Event for PanicOnce {
        type Route = AppNotification;
        type Source = AppSource;
        type Options = ();
    }
    impl Drop for PanicOnce {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
            if self.panic.replace(false) {
                panic!("payload destructor failed");
            }
        }
    }
    let panic = Rc::new(Cell::new(true));
    let drops = Rc::new(Cell::new(0));
    let queue = NotificationQueue::default();
    queue
        .reserve(capacity(
            2,
            2 * size_of::<PanicOnce>(),
            core::mem::align_of::<PanicOnce>(),
        ))
        .unwrap();
    for _ in 0..2 {
        queue
            .push_app(PanicOnce {
                panic: panic.clone(),
                drops: drops.clone(),
            })
            .unwrap();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = flush(&queue, 2, |_| {
            queue
                .push_app(Owned {
                    value: 1,
                    drops: drops.clone(),
                })
                .unwrap();
        });
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 3);
    let allocations = tracked_allocations(|| {
        queue.push_app(Empty).unwrap();
        assert_eq!(flush(&queue, 1, |_| ()), Ok(1));
    });
    assert_eq!(allocations, 0);
}
