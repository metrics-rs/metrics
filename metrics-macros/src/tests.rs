use syn::parse_quote;
use syn::{Expr, ExprPath};

use super::*;

#[test]
fn test_get_describe_code() {
    // Basic registration.
    let stream = get_describe_code(
        "mytype",
        parse_quote! { "mykeyname" },
        None,
        parse_quote! { "a counter" },
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "recorder . describe_mytype (\"mykeyname\" . into () , None , \"a counter\" . into ()) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_describe_code_with_qualified_unit_rooted() {
    // Now with unit.
    let units: ExprPath = parse_quote! { ::metrics::Unit::Nanoseconds };
    let stream = get_describe_code(
        "mytype",
        parse_quote! { "mykeyname" },
        Some(Expr::Path(units)),
        parse_quote! { "a counter" },
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "recorder . describe_mytype (\"mykeyname\" . into () , Some (:: metrics :: Unit :: Nanoseconds) , \"a counter\" . into ()) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_describe_code_with_qualified_unit() {
    // Now with unit.
    let units: ExprPath = parse_quote! { metrics::Unit::Nanoseconds };
    let stream = get_describe_code(
        "mytype",
        parse_quote! { "mykeyname" },
        Some(Expr::Path(units)),
        parse_quote! { "a counter" },
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "recorder . describe_mytype (\"mykeyname\" . into () , Some (metrics :: Unit :: Nanoseconds) , \"a counter\" . into ()) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_describe_code_with_relative_unit() {
    // Now with unit.
    let units: ExprPath = parse_quote! { Unit::Nanoseconds };
    let stream = get_describe_code(
        "mytype",
        parse_quote! { "mykeyname" },
        Some(Expr::Path(units)),
        parse_quote! { "a counter" },
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "recorder . describe_mytype (\"mykeyname\" . into () , Some (Unit :: Nanoseconds) , \"a counter\" . into ()) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_describe_code_with_constants() {
    // Basic registration.
    let stream =
        get_describe_code("mytype", parse_quote! { KEY_NAME }, None, parse_quote! { COUNTER_DESC });

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "recorder . describe_mytype (KEY_NAME . into () , None , COUNTER_DESC . into ()) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_describe_code_with_constants_and_with_qualified_unit() {
    // Now with unit.
    let units: ExprPath = parse_quote! { metrics::Unit::Nanoseconds };
    let stream = get_describe_code(
        "mytype",
        parse_quote! { KEY_NAME },
        Some(Expr::Path(units)),
        parse_quote! { COUNTER_DESC },
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "recorder . describe_mytype (KEY_NAME . into () , Some (metrics :: Unit :: Nanoseconds) , COUNTER_DESC . into ()) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_describe_code_with_constants_and_with_qualified_unit_rooted() {
    // Now with unit.
    let units: ExprPath = parse_quote! { ::metrics::Unit::Nanoseconds };
    let stream = get_describe_code(
        "mytype",
        parse_quote! { KEY_NAME },
        Some(Expr::Path(units)),
        parse_quote! { COUNTER_DESC },
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "recorder . describe_mytype (KEY_NAME . into () , Some (:: metrics :: Unit :: Nanoseconds) , COUNTER_DESC . into ()) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_describe_code_with_constants_and_with_relative_unit() {
    // Now with unit.
    let units: ExprPath = parse_quote! { Unit::Nanoseconds };
    let stream = get_describe_code(
        "mytype",
        parse_quote! { KEY_NAME },
        Some(Expr::Path(units)),
        parse_quote! { COUNTER_DESC },
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "recorder . describe_mytype (KEY_NAME . into () , Some (Unit :: Nanoseconds) , COUNTER_DESC . into ()) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_register_static_name_no_labels() {
    let stream = get_register_and_op_code::<bool>("mytype", parse_quote! {"mykeyname"}, None, None);

    let expected = concat!(
        "{ ",
        "static METRIC_NAME : & 'static str = \"mykeyname\" ; ",
        "static METRIC_KEY : :: metrics :: Key = :: metrics :: Key :: from_static_name (METRIC_NAME) ; ",
        ":: metrics :: recorder () . register_mytype (& METRIC_KEY) ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_register_static_name_static_inline_labels() {
    let labels = Labels::Inline(vec![(parse_quote! { "key1" }, parse_quote! { "value1" })]);
    let stream =
        get_register_and_op_code::<bool>("mytype", parse_quote! {"mykeyname"}, Some(labels), None);

    let expected = concat!(
        "{ ",
        "static METRIC_NAME : & 'static str = \"mykeyname\" ; ",
        "static METRIC_LABELS : [:: metrics :: Label ; 1usize] = [:: metrics :: Label :: from_static_parts (\"key1\" , \"value1\")] ; ",
        "static METRIC_KEY : :: metrics :: Key = :: metrics :: Key :: from_static_parts (METRIC_NAME , & METRIC_LABELS) ; ",
        ":: metrics :: recorder () . register_mytype (& METRIC_KEY) ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_register_static_name_dynamic_inline_labels() {
    let labels = Labels::Inline(vec![(parse_quote! { "key1" }, parse_quote! { &value1 })]);
    let stream =
        get_register_and_op_code::<bool>("mytype", parse_quote! {"mykeyname"}, Some(labels), None);

    let expected = concat!(
        "{ ",
        "static METRIC_NAME : & 'static str = \"mykeyname\" ; ",
        "let key = :: metrics :: Key :: from_parts (METRIC_NAME , vec ! [:: metrics :: Label :: new (\"key1\" , & value1)]) ; ",
        ":: metrics :: recorder () . register_mytype (& key) ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

/// If there are dynamic labels - generate a direct invocation.
#[test]
fn test_get_register_and_op_code_register_static_name_existing_labels() {
    let stream = get_register_and_op_code::<bool>(
        "mytype",
        parse_quote! {"mykeyname"},
        Some(Labels::Existing(parse_quote! { mylabels })),
        None,
    );

    let expected = concat!(
        "{ ",
        "static METRIC_NAME : & 'static str = \"mykeyname\" ; ",
        "let key = :: metrics :: Key :: from_parts (METRIC_NAME , mylabels) ; ",
        ":: metrics :: recorder () . register_mytype (& key) ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_register_owned_name_no_labels() {
    let stream = get_register_and_op_code::<bool>(
        "mytype",
        parse_quote! { String::from("owned") },
        None,
        None,
    );

    let expected = concat!(
        "{ ",
        "let key = :: metrics :: Key :: from_name (String :: from (\"owned\")) ; ",
        ":: metrics :: recorder () . register_mytype (& key) ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_register_owned_name_static_inline_labels() {
    let labels = Labels::Inline(vec![(parse_quote! { "key1" }, parse_quote! { "value1" })]);
    let stream = get_register_and_op_code::<bool>(
        "mytype",
        parse_quote! { String::from("owned") },
        Some(labels),
        None,
    );

    let expected = concat!(
        "{ ",
        "static METRIC_LABELS : [:: metrics :: Label ; 1usize] = [:: metrics :: Label :: from_static_parts (\"key1\" , \"value1\")] ; ",
        "let key = :: metrics :: Key :: from_static_labels (String :: from (\"owned\") , & METRIC_LABELS) ; ",
        ":: metrics :: recorder () . register_mytype (& key) ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_register_owned_name_dynamic_inline_labels() {
    let labels = Labels::Inline(vec![(parse_quote! { "key1" }, parse_quote! { &value1 })]);
    let stream = get_register_and_op_code::<bool>(
        "mytype",
        parse_quote! { String::from("owned") },
        Some(labels),
        None,
    );

    let expected = concat!(
        "{ ",
        "let key = :: metrics :: Key :: from_parts (String :: from (\"owned\") , vec ! [:: metrics :: Label :: new (\"key1\" , & value1)]) ; ",
        ":: metrics :: recorder () . register_mytype (& key) ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

/// If there are dynamic labels - generate a direct invocation.
#[test]
fn test_get_register_and_op_code_register_owned_name_existing_labels() {
    let stream = get_register_and_op_code::<bool>(
        "mytype",
        parse_quote! { String::from("owned") },
        Some(Labels::Existing(parse_quote! { mylabels })),
        None,
    );

    let expected = concat!(
        "{ ",
        "let key = :: metrics :: Key :: from_parts (String :: from (\"owned\") , mylabels) ; ",
        ":: metrics :: recorder () . register_mytype (& key) ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_op_static_name_no_labels() {
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! {"mykeyname"},
        None,
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "static METRIC_NAME : & 'static str = \"mykeyname\" ; ",
        "static METRIC_KEY : :: metrics :: Key = :: metrics :: Key :: from_static_name (METRIC_NAME) ; ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let handle = recorder . register_mytype (& METRIC_KEY) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_op_static_name_static_inline_labels() {
    let labels = Labels::Inline(vec![(parse_quote! { "key1" }, parse_quote! { "value1" })]);
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! {"mykeyname"},
        Some(labels),
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "static METRIC_NAME : & 'static str = \"mykeyname\" ; ",
        "static METRIC_LABELS : [:: metrics :: Label ; 1usize] = [:: metrics :: Label :: from_static_parts (\"key1\" , \"value1\")] ; ",
        "static METRIC_KEY : :: metrics :: Key = :: metrics :: Key :: from_static_parts (METRIC_NAME , & METRIC_LABELS) ; ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let handle = recorder . register_mytype (& METRIC_KEY) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_op_static_name_dynamic_inline_labels() {
    let labels = Labels::Inline(vec![(parse_quote! { "key1" }, parse_quote! { &value1 })]);
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! {"mykeyname"},
        Some(labels),
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "static METRIC_NAME : & 'static str = \"mykeyname\" ; ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let key = :: metrics :: Key :: from_parts (METRIC_NAME , vec ! [:: metrics :: Label :: new (\"key1\" , & value1)]) ; ",
        "let handle = recorder . register_mytype (& key) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

/// If there are dynamic labels - generate a direct invocation.
#[test]
fn test_get_register_and_op_code_op_static_name_existing_labels() {
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! {"mykeyname"},
        Some(Labels::Existing(parse_quote! { mylabels })),
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "static METRIC_NAME : & 'static str = \"mykeyname\" ; ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let key = :: metrics :: Key :: from_parts (METRIC_NAME , mylabels) ; ",
        "let handle = recorder . register_mytype (& key) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_op_owned_name_no_labels() {
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! { String::from("owned") },
        None,
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let key = :: metrics :: Key :: from_name (String :: from (\"owned\")) ; ",
        "let handle = recorder . register_mytype (& key) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_op_owned_name_static_inline_labels() {
    let labels = Labels::Inline(vec![(parse_quote! { "key1" }, parse_quote! { "value1" })]);
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! { String::from("owned") },
        Some(labels),
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "static METRIC_LABELS : [:: metrics :: Label ; 1usize] = [:: metrics :: Label :: from_static_parts (\"key1\" , \"value1\")] ; ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let key = :: metrics :: Key :: from_static_labels (String :: from (\"owned\") , & METRIC_LABELS) ; ",
        "let handle = recorder . register_mytype (& key) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_op_owned_name_dynamic_inline_labels() {
    let labels = Labels::Inline(vec![(parse_quote! { "key1" }, parse_quote! { &value1 })]);
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! { String::from("owned") },
        Some(labels),
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let key = :: metrics :: Key :: from_parts (String :: from (\"owned\") , vec ! [:: metrics :: Label :: new (\"key1\" , & value1)]) ; ",
        "let handle = recorder . register_mytype (& key) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

/// If there are dynamic labels - generate a direct invocation.
#[test]
fn test_get_register_and_op_code_op_owned_name_existing_labels() {
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! { String::from("owned") },
        Some(Labels::Existing(parse_quote! { mylabels })),
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let key = :: metrics :: Key :: from_parts (String :: from (\"owned\") , mylabels) ; ",
        "let handle = recorder . register_mytype (& key) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_register_and_op_code_op_owned_name_constant_key_labels() {
    let stream = get_register_and_op_code(
        "mytype",
        parse_quote! { String::from("owned") },
        Some(Labels::Inline(vec![(parse_quote! { LABEL_KEY }, parse_quote! { "some_val" })])),
        Some(("myop", quote! { 1 })),
    );

    let expected = concat!(
        "{ ",
        "if let Some (recorder) = :: metrics :: try_recorder () { ",
        "let key = :: metrics :: Key :: from_parts (String :: from (\"owned\") , vec ! [:: metrics :: Label :: new (LABEL_KEY , \"some_val\")]) ; ",
        "let handle = recorder . register_mytype (& key) ; ",
        "handle . myop (1) ; ",
        "} ",
        "}",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_labels_to_quoted_existing_labels() {
    let labels = Labels::Existing(Expr::Path(parse_quote! { mylabels }));
    let stream = labels_to_quoted(&labels);
    let expected = "mylabels";
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_labels_to_quoted_inline_labels() {
    let labels = Labels::Inline(vec![
        (parse_quote! {"mylabel1"}, parse_quote! { mylabel1 }),
        (parse_quote! {"mylabel2"}, parse_quote! { "mylabel2" }),
    ]);
    let stream = labels_to_quoted(&labels);
    let expected = concat!(
        "vec ! [",
        ":: metrics :: Label :: new (\"mylabel1\" , mylabel1) , ",
        ":: metrics :: Label :: new (\"mylabel2\" , \"mylabel2\")",
        "]"
    );
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_labels_to_quoted_inline_labels_empty() {
    let labels = Labels::Inline(vec![]);
    let stream = labels_to_quoted(&labels);
    let expected = "vec ! []";
    assert_eq!(stream.to_string(), expected);
}
