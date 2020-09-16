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
        "metrics :: Key :: Owned ( metrics :: KeyData :: from_name ( \"mykeyname\" ) ) , ",
        "None ",
        ") ; ",
        "} }",
    );

    assert_eq!(stream.to_string(), expected);
}

/// If there are no dynamic labels - generate an invocation with caching.
#[test]
fn test_get_expanded_callsite_fast_path() {
    let stream = get_expanded_callsite(
        "mytype",
        "myop",
        Key::NotScoped(parse_quote! {"mykeyname"}),
        None,
        quote! { 1 },
    );

    let expected = concat!(
        "{ ",
        "static CACHED_KEY : metrics :: OnceKeyData = metrics :: OnceKeyData :: new ( ) ; ",
        "if let Some ( recorder ) = metrics :: try_recorder ( ) { ",
        "let key = CACHED_KEY . get_or_init ( || { ",
        "metrics :: KeyData :: from_name ( \"mykeyname\" ) ",
        "} ) ; ",
        "recorder . myop_mytype ( metrics :: Key :: Borrowed ( & key ) , 1 ) ; ",
        "} }",
    );

    assert_eq!(stream.to_string(), expected);
}

/// If there are dynamic labels - generate a direct invocation.
#[test]
fn test_get_expanded_callsite_regular_path() {
    let stream = get_expanded_callsite(
        "mytype",
        "myop",
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Existing(parse_quote! { mylabels })),
        quote! { 1 },
    );

    let expected = concat!(
        "{ ",
        "if let Some ( recorder ) = metrics :: try_recorder ( ) { ",
        "recorder . myop_mytype ( ",
        "metrics :: Key :: Owned ( metrics :: KeyData :: from_name_and_labels ( \"mykeyname\" , mylabels ) ) , ",
        "1 ",
        ") ; ",
        "} }",
    );

    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_key_to_quoted_no_labels() {
    let stream = key_to_quoted(Key::NotScoped(parse_quote! {"mykeyname"}), None);
    let expected = "metrics :: KeyData :: from_name ( \"mykeyname\" )";
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_key_to_quoted_existing_labels() {
    let stream = key_to_quoted(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Existing(Expr::Path(parse_quote! { mylabels }))),
    );
    let expected = "metrics :: KeyData :: from_name_and_labels ( \"mykeyname\" , mylabels )";
    assert_eq!(stream.to_string(), expected);
}

/// Registration can only operate on static labels (i.e. labels baked into the
/// Key).
#[test]
fn test_key_to_quoted_inline_labels() {
    let stream = key_to_quoted(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Inline(vec![
            (parse_quote! {"mylabel1"}, parse_quote! { mylabel1 }),
            (parse_quote! {"mylabel2"}, parse_quote! { "mylabel2" }),
        ])),
    );
    let expected = concat!(
        "metrics :: KeyData :: from_name_and_labels ( \"mykeyname\" , vec ! [ ",
        "metrics :: Label :: new ( \"mylabel1\" , mylabel1 ) , ",
        "metrics :: Label :: new ( \"mylabel2\" , \"mylabel2\" ) ",
        "] )"
    );
    assert_eq!(stream.to_string(), expected);
}

#[test]
fn test_key_to_quoted_inline_labels_empty() {
    let stream = key_to_quoted(
        Key::NotScoped(parse_quote! {"mykeyname"}),
        Some(Labels::Inline(vec![])),
    );
    let expected = concat!(
        "metrics :: KeyData :: from_name_and_labels ( \"mykeyname\" , vec ! [ ",
        "] )"
    );
    assert_eq!(stream.to_string(), expected);
}
