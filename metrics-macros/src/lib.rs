extern crate proc_macro;

use self::proc_macro::TokenStream;

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Error, Parse, ParseStream, Result};
use syn::{parse::discouraged::Speculative, Lit};
use syn::{parse_macro_input, Expr, Token};

#[cfg(test)]
mod tests;

enum Labels {
    Existing(Expr),
    Inline(Vec<(Expr, Expr)>),
}

struct WithoutExpression {
    key: Expr,
    labels: Option<Labels>,
}

struct WithExpression {
    key: Expr,
    op_value: Expr,
    labels: Option<Labels>,
}

struct Description {
    key: Expr,
    unit: Option<Expr>,
    description: Expr,
}

impl Parse for WithoutExpression {
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = input.parse::<Expr>()?;
        let labels = parse_labels(&mut input)?;

        Ok(WithoutExpression { key, labels })
    }
}

impl Parse for WithExpression {
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = input.parse::<Expr>()?;

        input.parse::<Token![,]>()?;
        let op_value = input.parse::<Expr>()?;

        let labels = parse_labels(&mut input)?;

        Ok(WithExpression { key, op_value, labels })
    }
}

impl Parse for Description {
    fn parse(input: ParseStream) -> Result<Self> {
        let key = input.parse::<Expr>()?;

        // We accept two possible parameters: unit, and description.
        //
        // There is only one specific requirement that must be met, and that is that the || _must_
        // have a qualified path of either `metrics::Unit::...` or `Unit::..` for us to properly
        // distinguish it amongst the macro parameters.

        // Now try to read out the components.  We speculatively try to parse out a unit if it
        // exists, and otherwise we just look for the description.
        let unit = input
            .call(|s| {
                let forked = s.fork();
                forked.parse::<Token![,]>()?;

                let output = if let Ok(Expr::Path(path)) = forked.parse::<Expr>() {
                    let qname = path
                        .path
                        .segments
                        .iter()
                        .map(|x| x.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    if qname.starts_with("::metrics::Unit")
                        || qname.starts_with("metrics::Unit")
                        || qname.starts_with("Unit")
                    {
                        Some(Expr::Path(path))
                    } else {
                        None
                    }
                } else {
                    None
                };

                if output.is_some() {
                    s.advance_to(&forked);
                }

                Ok(output)
            })
            .ok()
            .flatten();

        input.parse::<Token![,]>()?;
        let description = input.parse::<Expr>()?;

        Ok(Description { key, unit, description })
    }
}

#[proc_macro]
pub fn describe_counter(input: TokenStream) -> TokenStream {
    let Description { key, unit, description } = parse_macro_input!(input as Description);

    get_describe_code("counter", key, unit, description).into()
}

#[proc_macro]
pub fn describe_gauge(input: TokenStream) -> TokenStream {
    let Description { key, unit, description } = parse_macro_input!(input as Description);

    get_describe_code("gauge", key, unit, description).into()
}

#[proc_macro]
pub fn describe_histogram(input: TokenStream) -> TokenStream {
    let Description { key, unit, description } = parse_macro_input!(input as Description);

    get_describe_code("histogram", key, unit, description).into()
}

#[proc_macro]
pub fn register_counter(input: TokenStream) -> TokenStream {
    let WithoutExpression { key, labels } = parse_macro_input!(input as WithoutExpression);

    get_register_and_op_code::<bool>("counter", key, labels, None).into()
}

#[proc_macro]
pub fn register_gauge(input: TokenStream) -> TokenStream {
    let WithoutExpression { key, labels } = parse_macro_input!(input as WithoutExpression);

    get_register_and_op_code::<bool>("gauge", key, labels, None).into()
}

#[proc_macro]
pub fn register_histogram(input: TokenStream) -> TokenStream {
    let WithoutExpression { key, labels } = parse_macro_input!(input as WithoutExpression);

    get_register_and_op_code::<bool>("histogram", key, labels, None).into()
}

#[proc_macro]
pub fn increment_counter(input: TokenStream) -> TokenStream {
    let WithoutExpression { key, labels } = parse_macro_input!(input as WithoutExpression);

    let op_value = quote! { 1 };

    get_register_and_op_code("counter", key, labels, Some(("increment", op_value))).into()
}

#[proc_macro]
pub fn counter(input: TokenStream) -> TokenStream {
    let WithExpression { key, op_value, labels } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("counter", key, labels, Some(("increment", op_value))).into()
}

#[proc_macro]
pub fn absolute_counter(input: TokenStream) -> TokenStream {
    let WithExpression { key, op_value, labels } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("counter", key, labels, Some(("absolute", op_value))).into()
}

#[proc_macro]
pub fn increment_gauge(input: TokenStream) -> TokenStream {
    let WithExpression { key, op_value, labels } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("gauge", key, labels, Some(("increment", op_value))).into()
}

#[proc_macro]
pub fn decrement_gauge(input: TokenStream) -> TokenStream {
    let WithExpression { key, op_value, labels } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("gauge", key, labels, Some(("decrement", op_value))).into()
}

#[proc_macro]
pub fn gauge(input: TokenStream) -> TokenStream {
    let WithExpression { key, op_value, labels } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("gauge", key, labels, Some(("set", op_value))).into()
}

#[proc_macro]
pub fn histogram(input: TokenStream) -> TokenStream {
    let WithExpression { key, op_value, labels } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("histogram", key, labels, Some(("record", op_value))).into()
}

fn get_describe_code(
    metric_type: &str,
    name: Expr,
    unit: Option<Expr>,
    description: Expr,
) -> TokenStream2 {
    let describe_ident = format_ident!("describe_{}", metric_type);

    let unit = match unit {
        Some(e) => quote! { Some(#e) },
        None => quote! { None },
    };

    quote! {
        {
            // Only do this work if there's a recorder installed.
            if let Some(recorder) = ::metrics::try_recorder() {
                recorder.#describe_ident(#name.into(), #unit, #description.into());
            }
        }
    }
}

fn get_register_and_op_code<V>(
    metric_type: &str,
    name: Expr,
    labels: Option<Labels>,
    op: Option<(&'static str, V)>,
) -> TokenStream2
where
    V: ToTokens,
{
    let register_ident = format_ident!("register_{}", metric_type);
    let statics = generate_statics(&name, &labels);
    let (locals, metric_key) = generate_metric_key(&name, &labels);
    match op {
        Some((op_type, op_value)) => {
            let op_ident = format_ident!("{}", op_type);
            let op_value = if metric_type == "histogram" {
                quote! { ::metrics::__into_f64(#op_value) }
            } else {
                quote! { #op_value }
            };

            // We've been given values to actually use with the handle, so we actually check if a
            // recorder is installed before bothering to create a handle and everything.
            quote! {
                {
                    #statics
                    // Only do this work if there's a recorder installed.
                    if let Some(recorder) = ::metrics::try_recorder() {
                        #locals
                        let handle = recorder.#register_ident(#metric_key);
                        handle.#op_ident(#op_value);
                    }
                }
            }
        }
        None => {
            // If there's no values specified, we simply return the metric handle.
            quote! {
                {
                    #statics
                    #locals
                    ::metrics::recorder().#register_ident(#metric_key)
                }
            }
        }
    }
}

fn name_is_fast_path(name: &Expr) -> bool {
    if let Expr::Lit(lit) = name {
        return matches!(lit.lit, Lit::Str(_));
    }

    false
}

fn labels_are_fast_path(labels: &Labels) -> bool {
    match labels {
        Labels::Existing(_) => false,
        Labels::Inline(pairs) => {
            pairs.iter().all(|(k, v)| matches!((k, v), (Expr::Lit(_), Expr::Lit(_))))
        }
    }
}

fn generate_statics(name: &Expr, labels: &Option<Labels>) -> TokenStream2 {
    // Create the static for the name, if possible.
    let use_name_static = name_is_fast_path(name);
    let name_static = if use_name_static {
        quote! {
            static METRIC_NAME: &'static str = #name;
        }
    } else {
        quote! {}
    };

    // Create the static for the labels, if possible.
    let has_labels = labels.is_some();
    let use_labels_static = match labels.as_ref() {
        Some(labels) => labels_are_fast_path(labels),
        None => true,
    };

    let labels_static = match labels.as_ref() {
        Some(labels) => {
            if labels_are_fast_path(labels) {
                if let Labels::Inline(pairs) = labels {
                    let labels = pairs
                        .iter()
                        .map(
                            |(key, val)| quote! { ::metrics::Label::from_static_parts(#key, #val) },
                        )
                        .collect::<Vec<_>>();
                    let labels_len = labels.len();
                    let labels_len = quote! { #labels_len };

                    quote! {
                        static METRIC_LABELS: [::metrics::Label; #labels_len] = [#(#labels),*];
                    }
                } else {
                    quote! {}
                }
            } else {
                quote! {}
            }
        }
        None => quote! {},
    };

    let key_static = if use_name_static && use_labels_static {
        if has_labels {
            quote! {
                static METRIC_KEY: ::metrics::Key = ::metrics::Key::from_static_parts(METRIC_NAME, &METRIC_LABELS);
            }
        } else {
            quote! {
                static METRIC_KEY: ::metrics::Key = ::metrics::Key::from_static_name(METRIC_NAME);
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #name_static
        #labels_static
        #key_static
    }
}

fn generate_metric_key(name: &Expr, labels: &Option<Labels>) -> (TokenStream2, TokenStream2) {
    let use_name_static = name_is_fast_path(name);

    let has_labels = labels.is_some();
    let use_labels_static = match labels.as_ref() {
        Some(labels) => labels_are_fast_path(labels),
        None => true,
    };

    let mut key_name = quote! { &key };
    let locals = if use_name_static && use_labels_static {
        // Key is entirely static, so we can simply reference our generated statics.  They will be
        // inclusive of whether or not labels were specified.
        key_name = quote! { &METRIC_KEY };
        quote! {}
    } else if use_name_static && !use_labels_static {
        // The name is static, but we have labels which are not static.  Since `use_labels_static`
        // cannot be false unless labels _are_ specified, we know this unwrap is safe.
        let labels = labels.as_ref().unwrap();
        let quoted_labels = labels_to_quoted(labels);
        quote! {
            let key = ::metrics::Key::from_parts(METRIC_NAME, #quoted_labels);
        }
    } else if !use_name_static && !use_labels_static {
        // The name is not static, and neither are the labels. Since `use_labels_static`
        // cannot be false unless labels _are_ specified, we know this unwrap is safe.
        let labels = labels.as_ref().unwrap();
        let quoted_labels = labels_to_quoted(labels);
        quote! {
            let key = ::metrics::Key::from_parts(#name, #quoted_labels);
        }
    } else {
        // The name is not static, but the labels are.  This could technically mean that there
        // simply are no labels, so we have to discriminate in a slightly different way
        // to figure out the correct key.
        if has_labels {
            quote! {
                let key = ::metrics::Key::from_static_labels(#name, &METRIC_LABELS);
            }
        } else {
            quote! {
                let key = ::metrics::Key::from_name(#name);
            }
        }
    };

    (locals, key_name)
}

fn labels_to_quoted(labels: &Labels) -> proc_macro2::TokenStream {
    match labels {
        Labels::Inline(pairs) => {
            let labels =
                pairs.iter().map(|(key, val)| quote! { ::metrics::Label::new(#key, #val) });
            quote! { vec![#(#labels),*] }
        }
        Labels::Existing(e) => quote! { #e },
    }
}

fn parse_labels(input: &mut ParseStream) -> Result<Option<Labels>> {
    if input.is_empty() {
        return Ok(None);
    }

    if !input.peek(Token![,]) {
        // This is a hack to generate the proper error message for parsing the comma next without
        // actually parsing it and thus removing it from the parse stream.  Just makes the following
        // code a bit cleaner.
        input
            .parse::<Token![,]>()
            .map_err(|e| Error::new(e.span(), "expected labels, but comma not found"))?;
    }

    // Two possible states for labels: references to a label iterator, or key/value pairs.
    //
    // We check to see if we have the ", key =>" part, which tells us that we're taking in key/value
    // pairs.  If we don't have that, we check to see if we have a "`, <expr" part, which could us
    // getting handed a labels iterator.  The type checking for `IntoLabels` in `metrics::Recorder`
    // will do the heavy lifting from that point forward.
    if input.peek(Token![,]) && input.peek3(Token![=>]) {
        let mut labels = Vec::new();
        loop {
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let k = input.parse::<Expr>()?;
            input.parse::<Token![=>]>()?;
            let v = input.parse::<Expr>()?;

            labels.push((k, v));
        }

        return Ok(Some(Labels::Inline(labels)));
    }

    // Has to be an expression otherwise, or a trailing comma.
    input.parse::<Token![,]>()?;

    // Unless it was an expression - clear the trailing comma.
    if input.is_empty() {
        return Ok(None);
    }

    let existing = input.parse::<Expr>().map_err(|e| {
        Error::new(e.span(), "expected labels expression, but expression not found")
    })?;

    // Expression can end with a trailing comma, handle it.
    if input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
    }

    Ok(Some(Labels::Existing(existing)))
}
