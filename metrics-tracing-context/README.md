# metrics-tracing-context

A crate to use tracing context as metrics labels.

## Configuring field merge policies

By default, when a child span defines a field that its parent also defines, the child's
value wins. This behavior can be customized per field name using a `FieldMergePolicy`,
for example to compose hierarchical values across nested spans:

```rust
use metrics_tracing_context::{FieldMergePolicy, MetricsLayer};

let metrics_layer = MetricsLayer::new()
    .with_field_merge_policy("component", FieldMergePolicy::Append("."));
```

With this configuration, given a root span with `component = "analyzer"` and a nested
span with `component = "worker"`, metrics emitted within the nested span will carry
`component = "analyzer.worker"`.
