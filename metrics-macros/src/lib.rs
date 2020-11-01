extern crate proc_macro;

use self::proc_macro::TokenStream;

use lazy_static::lazy_static;
use proc_macro_hack::proc_macro_hack;
use quote::{format_ident, quote, ToTokens};
use regex::Regex;
use syn::parse::discouraged::Speculative;
use syn::parse::{Error, Parse, ParseStream, Result};
use syn::{parse_macro_input, Expr, LitStr, Token};

#[cfg(test)]
mod tests;

enum Labels {
    Existing(Expr),
    Inline(Vec<(LitStr, Expr)>),
}

struct WithoutExpression {
    key: LitStr,
    labels: Option<Labels>,
}

struct WithExpression {
    key: LitStr,
    op_value: Expr,
    labels: Option<Labels>,
}

struct Registration {
    key: LitStr,
    unit: Option<Expr>,
    description: Option<LitStr>,
    labels: Option<Labels>,
}

impl Parse for WithoutExpression {
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = read_key(&mut input)?;
        let labels = parse_labels(&mut input)?;

        Ok(WithoutExpression { key, labels })
    }
}

impl Parse for WithExpression {
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = read_key(&mut input)?;

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

impl Parse for Registration {
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = read_key(&mut input)?;

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

        Ok(Registration {
            key,
            unit,
            description,
            labels,
        })
    }
}

#[proc_macro_hack]
pub fn register_counter(input: TokenStream) -> TokenStream {
    let Registration {
        key,
        unit,
        description,
        labels,
    } = parse_macro_input!(input as Registration);

    get_expanded_registration("counter", key, unit, description, labels).into()
}

#[proc_macro_hack]
pub fn register_gauge(input: TokenStream) -> TokenStream {
    let Registration {
        key,
        unit,
        description,
        labels,
    } = parse_macro_input!(input as Registration);

    get_expanded_registration("gauge", key, unit, description, labels).into()
}

#[proc_macro_hack]
pub fn register_histogram(input: TokenStream) -> TokenStream {
    let Registration {
        key,
        unit,
        description,
        labels,
    } = parse_macro_input!(input as Registration);

    get_expanded_registration("histogram", key, unit, description, labels).into()
}

#[proc_macro_hack]
pub fn increment(input: TokenStream) -> TokenStream {
    let WithoutExpression { key, labels } = parse_macro_input!(input as WithoutExpression);

    let op_value = quote! { 1 };

    get_expanded_callsite("counter", "increment", key, labels, op_value).into()
}

#[proc_macro_hack]
pub fn counter(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_expanded_callsite("counter", "increment", key, labels, op_value).into()
}

#[proc_macro_hack]
pub fn gauge(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_expanded_callsite("gauge", "update", key, labels, op_value).into()
}

#[proc_macro_hack]
pub fn histogram(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_expanded_callsite("histogram", "record", key, labels, op_value).into()
}

fn get_expanded_registration(
    metric_type: &str,
    key: LitStr,
    unit: Option<Expr>,
    description: Option<LitStr>,
    labels: Option<Labels>,
) -> proc_macro2::TokenStream {
    let register_ident = format_ident!("register_{}", metric_type);
    let key = key_to_quoted(key, labels);

    let unit = match unit {
        Some(e) => quote! { Some(#e) },
        None => quote! { None },
    };

    let description = match description {
        Some(s) => quote! { Some(#s) },
        None => quote! { None },
    };

    quote! {
        {
            // Only do this work if there's a recorder installed.
            if let Some(recorder) = metrics::try_recorder() {
                // Registrations are fairly rare, don't attempt to cache here
                // and just use an owned ref.
                recorder.#register_ident(metrics::Key::Owned(#key), #unit, #description);
            }
        }
    }
}

fn get_expanded_callsite<V>(
    metric_type: &str,
    op_type: &str,
    key: LitStr,
    labels: Option<Labels>,
    op_values: V,
) -> proc_macro2::TokenStream
where
    V: ToTokens,
{
    // We use a helper method for histogram values to coerce into u64, but otherwise,
    // just pass through whatever the caller gave us.
    let op_values = if metric_type == "histogram" {
        quote! { metrics::__into_u64(#op_values) }
    } else {
        quote! { #op_values }
    };

    let op_ident = format_ident!("{}_{}", op_type, metric_type);

    let use_fast_path = can_use_fast_path(&labels);
    if use_fast_path {
        // We're on the fast path here, so we'll build our key, statically cache it,
        // and use a borrowed reference to it for this and future operations.
        let statics = match labels {
            Some(Labels::Inline(pairs)) => {
                let labels = pairs
                    .into_iter()
                    .map(|(key, val)| quote! { metrics::Label::from_static_parts(#key, #val) })
                    .collect::<Vec<_>>();
                let labels_len = labels.len();
                let labels_len = quote! { #labels_len };

                quote! {
                    static METRIC_NAME: metrics::NameParts = metrics::NameParts::from_static_name(#key);
                    static METRIC_LABELS: [metrics::Label; #labels_len] = [#(#labels),*];
                    static METRIC_KEY: metrics::KeyData =
                        metrics::KeyData::from_static_parts(&METRIC_NAME, &METRIC_LABELS);
                }
            }
            None => {
                quote! {
                    static METRIC_NAME: metrics::NameParts = metrics::NameParts::from_static_name(#key);
                    static METRIC_KEY: metrics::KeyData =
                        metrics::KeyData::from_static_name(&METRIC_NAME);
                }
            }
            _ => unreachable!("use_fast_path == true, but found expression-based labels"),
        };

        quote! {
            {
                #statics

                // Only do this work if there's a recorder installed.
                if let Some(recorder) = metrics::try_recorder() {
                    recorder.#op_ident(metrics::Key::Borrowed(&METRIC_KEY), #op_values);
                }
            }
        }
    } else {
        // We're on the slow path, so we allocate, womp.
        let key = key_to_quoted(key, labels);
        quote! {
            {
                // Only do this work if there's a recorder installed.
                if let Some(recorder) = metrics::try_recorder() {
                    recorder.#op_ident(metrics::Key::Owned(#key), #op_values);
                }
            }
        }
    }
}

fn can_use_fast_path(labels: &Option<Labels>) -> bool {
    match labels {
        None => true,
        Some(labels) => match labels {
            Labels::Existing(_) => false,
            Labels::Inline(pairs) => pairs.iter().all(|(_, v)| matches!(v, Expr::Lit(_))),
        },
    }
}

fn read_key(input: &mut ParseStream) -> Result<LitStr> {
    let key = input.parse::<LitStr>()?;
    let inner = key.value();

    lazy_static! {
        static ref RE: Regex = Regex::new("^[a-zA-Z][a-zA-Z0-9_:\\.]*$").unwrap();
    }
    if !RE.is_match(&inner) {
        return Err(Error::new(
            key.span(),
            "metric name must match ^[a-zA-Z][a-zA-Z0-9_:.]*$",
        ));
    }

    Ok(key)
}

fn key_to_quoted(name: LitStr, labels: Option<Labels>) -> proc_macro2::TokenStream {
    match labels {
        None => quote! { metrics::KeyData::from_name(#name) },
        Some(labels) => match labels {
            Labels::Inline(pairs) => {
                let labels = pairs
                    .into_iter()
                    .map(|(key, val)| quote! { metrics::Label::new(#key, #val) });
                quote! { metrics::KeyData::from_parts(#name, vec![#(#labels),*]) }
            }
            Labels::Existing(e) => quote! { metrics::KeyData::from_parts(#name, #e) },
        },
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
