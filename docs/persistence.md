# Persistence

Enable the `persistence` feature to use `PersistencePlugin` with a storage backend.

## Automatic saves

Configure the interval with `autosave(interval)`. Integer arguments are milliseconds; `core::time::Duration` and `std::time::Duration` specify a duration directly.

```rust
use core::time::Duration;
use mirui::core::{persistence::PersistencePlugin, storage::MemoryStorage};

let plugin = PersistencePlugin::new(MemoryStorage::new()).autosave(1000);
let plugin = plugin.autosave(Duration::from_secs(2));
```

Periodic saving is disabled unless configured. Zero disables it again. Negative or unrepresentable integer intervals emit a warning and leave the previous configuration unchanged. Suspend and quit still attempt to save registered data.

Install a clock plugin before the persistence plugin to enable periodic saves. Due saves run at a persistence scheduling boundary, not on a separate timer thread. Submillisecond intervals retain their precision but cannot schedule saves more frequently than those boundaries.
