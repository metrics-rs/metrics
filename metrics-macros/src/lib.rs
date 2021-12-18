extern crate proc_macro;

use self::proc_macro::TokenStream;

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Error, Parse, ParseStream, Result};
use syn::{parse::discouraged::Speculative, Lit};
use syn::{parse_macro_input, Expr, LitStr, Token};

#[cfg(test)]
mod tests;

enum Labels {
    Existing(Expr),
    Inline(Vec<(LitStr, Expr)>),
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
    description: Option<LitStr>,
    labels: Option<Labels>,
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
        let op_value: Expr = input.parse()?;

        let labels = parse_labels(&mut input)?;

        Ok(WithExpression {
            key,
            op_value,
            labels,
        })
    }
}

impl Parse for Description {
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = input.parse::<Expr>()?;

        // We accept three possible parameters: unit, description, and labels.
        //
        // If our first parameter is a literal string, we either have the description and no labels,
        // or a description and labels.  Peek at the trailing token after the description to see if
        // we need to keep parsing.

        // This may or may not be the start of labels, if the description has been omitted, so
        // we hold on to it until we can make sure nothing else is behind it, or if it's a full
        // fledged set of labels.
        let (unit, description, labels) = if input.peek(Token![,]) && input.peek3(Token![=>]) {
            // We have a ", <something> =>" pattern, which can only be labels, so we have no
            // unit or description.
            let labels = parse_labels(&mut input)?;

            (None, None, labels)
        } else if input.peek(Token![,]) && input.peek2(LitStr) {
            // We already know we're not working with labels only, and if we have ", <literal
            // string>" then we have to at least have a description, possibly with labels.
            input.parse::<Token![,]>()?;
            let description = input.parse::<LitStr>().ok();
            let labels = parse_labels(&mut input)?;
            (None, description, labels)
        } else if input.peek(Token![,]) {
            // We may or may not have anything left to parse here, but it could also be any
            // combination of unit + description and/or labels.
            //
            // We speculatively try and parse an expression from the buffer, and see if we can match
            // it to the qualified name of the Unit enum.  We run all of the other normal parsing
            // after that for description and labels.
            let forked = input.fork();
            forked.parse::<Token![,]>()?;

            let unit = if let Ok(Expr::Path(path)) = forked.parse::<Expr>() {
                let qname = path
                    .path
                    .segments
                    .iter()
                    .map(|x| x.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                if qname.starts_with("metrics::Unit") || qname.starts_with("Unit") {
                    Some(Expr::Path(path))
                } else {
                    None
                }
            } else {
                None
            };

            // If we succeeded, advance the main parse stream up to where the fork left off.
            if unit.is_some() {
                input.advance_to(&forked);
            }

            // We still have to check for a possible description.
            let description =
                if input.peek(Token![,]) && input.peek2(LitStr) && !input.peek3(Token![=>]) {
                    input.parse::<Token![,]>()?;
                    input.parse::<LitStr>().ok()
                } else {
                    None
                };

            let labels = parse_labels(&mut input)?;
            (unit, description, labels)
        } else {
            (None, None, None)
        };

        Ok(Description {
            key,
            unit,
            description,
            labels,
        })
    }
}

#[proc_macro]
pub fn describe_counter(input: TokenStream) -> TokenStream {
    let Description {
        key,
        unit,
        description,
        labels,
    } = parse_macro_input!(input as Description);

    get_describe_code("counter", key, unit, description, labels).into()
}

#[proc_macro]
pub fn describe_gauge(input: TokenStream) -> TokenStream {
    let Description {
        key,
        unit,
        description,
        labels,
    } = parse_macro_input!(input as Description);

    get_describe_code("gauge", key, unit, description, labels).into()
}

#[proc_macro]
pub fn describe_histogram(input: TokenStream) -> TokenStream {
    let Description {
        key,
        unit,
        description,
        labels,
    } = parse_macro_input!(input as Description);

    get_describe_code("histogram", key, unit, description, labels).into()
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
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("counter", key, labels, Some(("increment", op_value))).into()
}

#[proc_macro]
pub fn absolute_counter(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("counter", key, labels, Some(("absolute", op_value))).into()
}

#[proc_macro]
pub fn increment_gauge(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("gauge", key, labels, Some(("increment", op_value))).into()
}

#[proc_macro]
pub fn decrement_gauge(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("gauge", key, labels, Some(("decrement", op_value))).into()
}

#[proc_macro]
pub fn gauge(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("gauge", key, labels, Some(("set", op_value))).into()
}

#[proc_macro]
pub fn histogram(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_register_and_op_code("histogram", key, labels, Some(("record", op_value))).into()
}

fn get_describe_code(
    metric_type: &str,
    name: Expr,
    unit: Option<Expr>,
    description: Option<LitStr>,
    labels: Option<Labels>,
) -> TokenStream2 {
    let describe_ident = format_ident!("describe_{}", metric_type);

    let unit = match unit {
        Some(e) => quote! { Some(#e) },
        None => quote! { None },
    };

    let description = match description {
        Some(s) => quote! { Some(#s) },
        None => quote! { None },
    };

    let statics = generate_statics(&name, &labels);
    let (locals, metric_key) = generate_metric_key(&name, &labels);
    quote! {
        {
            #statics
            // Only do this work if there's a recorder installed.
            if let Some(recorder) = metrics::try_recorder() {
                #locals
                recorder.#describe_ident(#metric_key, #unit, #description);
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
                quote! { metrics::__into_f64(#op_value) }
            } else {
                quote! { #op_value }
            };

            // We've been given values to actually use with the handle, so we actually check if a
            // recorder is installed before bothering to create a handle and everything.
            quote! {
                {
                    #statics
                    // Only do this work if there's a recorder installed.
                    if let Some(recorder) = metrics::try_recorder() {
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
                    metrics::recorder().#register_ident(#metric_key)
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
        Labels::Inline(pairs) => pairs.iter().all(|(_, v)| matches!(v, Expr::Lit(_))),
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
                        .map(|(key, val)| quote! { metrics::Label::from_static_parts(#key, #val) })
                        .collect::<Vec<_>>();
                    let labels_len = labels.len();
                    let labels_len = quote! { #labels_len };

                    quote! {
                        static METRIC_LABELS: [metrics::Label; #labels_len] = [#(#labels),*];
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
                static METRIC_KEY: metrics::Key = metrics::Key::from_static_parts(METRIC_NAME, &METRIC_LABELS);
            }
        } else {
            quote! {
                static METRIC_KEY: metrics::Key = metrics::Key::from_static_name(METRIC_NAME);
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
            let key = metrics::Key::from_parts(METRIC_NAME, #quoted_labels);
        }
    } else if !use_name_static && !use_labels_static {
        // The name is not static, and neither are the labels. Since `use_labels_static`
        // cannot be false unless labels _are_ specified, we know this unwrap is safe.
        let labels = labels.as_ref().unwrap();
        let quoted_labels = labels_to_quoted(labels);
        quote! {
            let key = metrics::Key::from_parts(#name, #quoted_labels);
        }
    } else {
        // The name is not static, but the labels are.  This could technically mean that there
        // simply are no labels, so we have to discriminate in a slightly different way
        // to figure out the correct key.
        if has_labels {
            quote! {
                let key = metrics::Key::from_static_labels(#name, &METRIC_LABELS);
            }
        } else {
            quote! {
                let key = metrics::Key::from_name(#name);
            }
        }
    };

    (locals, key_name)
}

fn labels_to_quoted(labels: &Labels) -> proc_macro2::TokenStream {
    match labels {
        Labels::Inline(pairs) => {
            let labels = pairs
                .iter()
                .map(|(key, val)| quote! { metrics::Label::new(#key, #val) });
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
    if input.peek(Token![,]) && input.peek2(LitStr) && input.peek3(Token![=>]) {
        let mut labels = Vec::new();
        loop {
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let lkey: LitStr = input.parse()?;
            input.parse::<Token![=>]>()?;
            let lvalue: Expr = input.parse()?;

            labels.push((lkey, lvalue));
        }

        return Ok(Some(Labels::Inline(labels)));
    }

    // Has to be an expression otherwise, or a trailing comma.
    input.parse::<Token![,]>()?;

    // Unless it was an expression - clear the trailing comma.
    if input.is_empty() {
        return Ok(None);
    }

    let lvalue: Expr = input.parse().map_err(|e| {
        Error::new(
            e.span(),
            "expected label expression, but expression not found",
        )
    })?;

    // Expression can end with a trailing comma, handle it.
    if input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
    }

    Ok(Some(Labels::Existing(lvalue)))
}
