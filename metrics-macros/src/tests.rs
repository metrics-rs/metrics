use syn::parse_quote;

use super::*;

#[test]
fn test_quote_key_name_scoped() {
    let stream = quote_key_name(Key::Scoped(parse_quote! {"qwerty"}));
    let expected =
        "format ! ( \"{}.{}\" , std :: module_path ! ( ) . replace ( \"::\" , \".\" ) , \"qwerty\" )";
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_quote_key_name_not_scoped() {
    let stream = quote_key_name(Key::NotScoped(parse_quote! {"qwerty"}));
    let expected = "\"qwerty\"";
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_get_expanded_registration() {
    let stream = get_expanded_registration(
        "mytype",
        Key::NotScoped(parse_quote! {"mykeyname"}),
        None,
        None,
    );

    let expected = concat!(
        "{ if let Some ( recorder ) = metrics :: try_recorder ( ) { ",
        "recorder . register_mytype ( ",
        "metrics :: Key :: from_name ( \"mykeyname\" ) , ",
        "None ",
        ") ; ",
        "} }",
    );

    assert_eq!(stream.to_string(), expected);
}

/// If there are no dynamic labels - we generate the static invocation.
#[test]
fn test_get_expanded_callsite_no_dynamic_labels() {
    let stream = get_expanded_callsite(
        "mytype",
        "myop",
        Key::NotScoped(parse_quote! {"mykeyname"}),
        None,
        quote! { 1 },
    );

    let expected = concat!(
        "{ ",
        "static METRICS_INIT : metrics :: OnceIdentifier = metrics :: OnceIdentifier :: new ( ) ; ",
        "if let Some ( recorder ) = metrics :: try_recorder ( ) { ",
        "let id = METRICS_INIT . get_or_init ( || { ",
        "recorder . register_mytype ( metrics :: Key :: from_name ( \"mykeyname\" ) , None ) } ) ; ",
        "recorder . myop_mytype ( id , 1 ) ; ",
        "} }",
    );

    assert_eq!(stream.to_string(), expected);
}

/// If there were any dynamic labels - we generate the dynamic invocation.
#[test]
fn test_get_expanded_callsite_with_dynamic_labels() {
    let stream = get_expanded_callsite(
        "mytype",
        "myop",
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Existing(parse_quote! { mylabels })),
        quote! { 1 },
    );

    let expected = concat!(
        "{ if let Some ( recorder ) = metrics :: try_recorder ( ) { ",
        "recorder . myop_dynamic_mytype ( ",
        "metrics :: Key :: from_name ( \"mykeyname\" ) , ",
        "1 , ",
        "metrics :: IntoLabels :: into_labels ( mylabels ) ",
        ") ; } }",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_prepare_quoted_registration_no_labels() {
    let stream = prepare_quoted_registration(Key::NotScoped(parse_quote! {"mykeyname"}), None);
    let expected = "metrics :: Key :: from_name ( \"mykeyname\" )";
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_prepare_quoted_registration_existing_labels() {
    let stream = prepare_quoted_registration(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Existing(Expr::Path(parse_quote! { mylabels }))),
    );
    let expected = "metrics :: Key :: from_name_and_labels ( \"mykeyname\" , mylabels )";
    assert_eq!(stream.to_string(), expected);
}

/// Registration can only operate on static labels (i.e. labels baked into the
/// Key).
#[test]
fn test_prepare_quoted_registration_inline_labels() {
    let stream = prepare_quoted_registration(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Inline(vec![
            (parse_quote! {"mylabel1"}, parse_quote! { mylabel1 }),
            (parse_quote! {"mylabel2"}, parse_quote! { "mylabel2" }),
        ])),
    );
    let expected = concat!(
        "metrics :: Key :: from_name_and_labels ( \"mykeyname\" , vec ! [ ",
        "metrics :: Label :: new ( \"mylabel1\" , mylabel1 ) , ",
        "metrics :: Label :: new ( \"mylabel2\" , \"mylabel2\" ) ",
        "] )"
    );
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_prepare_quoted_registration_inline_labels_empty() {
    let stream = prepare_quoted_registration(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Inline(vec![])),
    );
    let expected = concat!(
        "metrics :: Key :: from_name_and_labels ( \"mykeyname\" , vec ! [ ",
        "] )"
    );
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_prepare_quoted_op_no_labels() {
    let (key_stream, dynamic_labels_stream) =
        prepare_quoted_op(Key::NotScoped(parse_quote! {"mykeyname"}), None);

    let expected_key = "metrics :: Key :: from_name ( \"mykeyname\" )";

    assert_eq!(key_stream.to_string(), expected_key);
    assert!(dynamic_labels_stream.is_none());
}

/// In the op invocation, existing labels are always considered dynamic labels.
#[test]
fn test_prepare_quoted_op_existing_labels() {
    let (key_stream, dynamic_labels_stream) = prepare_quoted_op(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Existing(Expr::Path(parse_quote! { mylabels }))),
    );

    let expected_key = "metrics :: Key :: from_name ( \"mykeyname\" )";
    let expected_dynamic_labels = "metrics :: IntoLabels :: into_labels ( mylabels )";

    assert_eq!(key_stream.to_string(), expected_key);
    assert_eq!(
        dynamic_labels_stream.unwrap().to_string(),
        expected_dynamic_labels
    );
}

#[test]
fn test_prepare_quoted_op_inline_labels_static_only() {
    let (key_stream, dynamic_labels_stream) = prepare_quoted_op(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Inline(vec![
            (
                parse_quote! {"my_static_label_1"},
                Expr::Lit(syn::parse_quote! {"my_static_val_1"}),
            ),
            (
                parse_quote! {"my_static_label_2"},
                Expr::Lit(syn::parse_quote! {"my_static_val_2"}),
            ),
        ])),
    );

    let expected_key = concat!(
        "metrics :: Key :: from_name_and_labels ( \"mykeyname\" , vec ! [ ",
        "metrics :: Label :: new ( \"my_static_label_1\" , \"my_static_val_1\" ) , ",
        "metrics :: Label :: new ( \"my_static_label_2\" , \"my_static_val_2\" ) ",
        "] )",
    );

    assert_eq!(key_stream.to_string(), expected_key);
    assert!(dynamic_labels_stream.is_none());
}

#[test]
fn test_prepare_quoted_op_inline_labels_dynamic_only() {
    let (key_stream, dynamic_labels_stream) = prepare_quoted_op(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Inline(vec![
            (
                parse_quote! {"my_dynamic_label_1"},
                Expr::Path(syn::parse_quote! {my_dynamic_val_1}),
            ),
            (
                parse_quote! {"my_dynamic_label_2"},
                Expr::Path(syn::parse_quote! {my_dynamic_val_2}),
            ),
        ])),
    );

    let expected_key = concat!(
        "metrics :: Key :: from_name_and_labels ( \"mykeyname\" , vec ! [ ",
        "] )",
    );
    let expected_dynamic_labels = concat!(
        "vec ! [ ",
        "metrics :: Label :: new ( \"my_dynamic_label_1\" , my_dynamic_val_1 ) , ",
        "metrics :: Label :: new ( \"my_dynamic_label_2\" , my_dynamic_val_2 ) ",
        "]",
    );

    assert_eq!(key_stream.to_string(), expected_key);
    assert_eq!(
        dynamic_labels_stream.unwrap().to_string(),
        expected_dynamic_labels
    );
}

#[test]
fn test_prepare_quoted_op_inline_labels_static_and_dynamic() {
    let (key_stream, dynamic_labels_stream) = prepare_quoted_op(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Inline(vec![
            (
                parse_quote! {"my_static_label_1"},
                Expr::Lit(syn::parse_quote! {"my_static_val_1"}),
            ),
            (
                parse_quote! {"my_dynamic_label_1"},
                Expr::Path(syn::parse_quote! {my_dynamic_val_1}),
            ),
            (
                parse_quote! {"my_static_label_2"},
                Expr::Lit(syn::parse_quote! {"my_static_val_2"}),
            ),
            (
                parse_quote! {"my_dynamic_label_2"},
                Expr::Path(syn::parse_quote! {my_dynamic_val_2}),
            ),
        ])),
    );

    let expected_key = concat!(
        "metrics :: Key :: from_name_and_labels ( \"mykeyname\" , vec ! [ ",
        "metrics :: Label :: new ( \"my_static_label_1\" , \"my_static_val_1\" ) , ",
        "metrics :: Label :: new ( \"my_static_label_2\" , \"my_static_val_2\" ) ",
        "] )",
    );
    let expected_dynamic_labels = concat!(
        "vec ! [ ",
        "metrics :: Label :: new ( \"my_dynamic_label_1\" , my_dynamic_val_1 ) , ",
        "metrics :: Label :: new ( \"my_dynamic_label_2\" , my_dynamic_val_2 ) ",
        "]",
    );

    assert_eq!(key_stream.to_string(), expected_key);
    assert_eq!(
        dynamic_labels_stream.unwrap().to_string(),
        expected_dynamic_labels
    );
}
