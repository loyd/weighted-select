# weighted-select

Usage:
```rust
use weighted_select::{self, IncompleteSelect};

let select = weighted_select::new()
    .append(fetch_from_a, 5)
    .append(fetch_from_b, 2)
    .append(fetch_from_c, 3)
    .build();
```

It produces a stream that combines three underlying streams (`fetch_from_*`) and polls them according to their weights (`5`, `2`, `3`). Each stream will be polled at most `weight` times consecutively.
