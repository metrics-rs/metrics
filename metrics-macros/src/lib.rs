extern crate proc_macro;

use self::proc_macro::TokenStream;

use std::iter::FromIterator;

use proc_macro_hack::proc_macro_hack;
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Parse, ParseStream, Result};
use syn::{parse_macro_input, Expr, LitStr, Token};

struct WithoutExpression {
    key: LitStr,
    labels: Vec<(LitStr, Expr)>,
}

struct WithExpression {
    key: LitStr,
    op_value: Expr,
    labels: Vec<(LitStr, Expr)>,
}

struct Registration {
    key: LitStr,
    desc: LitStr,
    labels: Vec<(LitStr, Expr)>,
}

impl Parse for WithoutExpression {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: LitStr = input.parse()?;

        let mut labels = Vec::new();
        loop {
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            let lkey: LitStr = input.parse()?;
            input.parse::<Token![=>]>()?;
            let lvalue: Expr = input.parse()?;

            labels.push((lkey, lvalue));
        }
        Ok(WithoutExpression { key, labels })
    }
}

impl Parse for WithExpression {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let op_value: Expr = input.parse()?;

        let mut labels = Vec::new();
        loop {
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            let lkey: LitStr = input.parse()?;
            input.parse::<Token![=>]>()?;
            let lvalue: Expr = input.parse()?;

            labels.push((lkey, lvalue));
        }
        Ok(WithExpression {
            key,
            op_value,
            labels,
        })
    }
}

impl Parse for Registration {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let desc: LitStr = input.parse()?;

        let mut labels = Vec::new();
        loop {
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            let lkey: LitStr = input.parse()?;
            input.parse::<Token![=>]>()?;
            let lvalue: Expr = input.parse()?;

            labels.push((lkey, lvalue));
        }
        Ok(Registration { key, desc, labels })
    }
}

#[proc_macro_hack]
pub fn register_counter(input: TokenStream) -> TokenStream {
    let Registration { key, desc, labels } = parse_macro_input!(input as Registration);

    get_expanded_registration("counter", key, desc, labels)
}

#[proc_macro_hack]
pub fn register_gauge(input: TokenStream) -> TokenStream {
    let Registration { key, desc, labels } = parse_macro_input!(input as Registration);

    get_expanded_registration("gauge", key, desc, labels)
}

#[proc_macro_hack]
pub fn register_histogram(input: TokenStream) -> TokenStream {
    let Registration { key, desc, labels } = parse_macro_input!(input as Registration);

    get_expanded_registration("histogram", key, desc, labels)
}

#[proc_macro_hack]
pub fn increment(input: TokenStream) -> TokenStream {
    let WithoutExpression { key, labels } = parse_macro_input!(input as WithoutExpression);

    let op_value = quote! { 1 };

    get_expanded_callsite("counter", "increment", key, labels, op_value)
}

#[proc_macro_hack]
pub fn counter(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_expanded_callsite("counter", "increment", key, labels, op_value)
}

#[proc_macro_hack]
pub fn gauge(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_expanded_callsite("gauge", "update", key, labels, op_value)
}

#[proc_macro_hack]
pub fn histogram(input: TokenStream) -> TokenStream {
    let WithExpression {
        key,
        op_value,
        labels,
    } = parse_macro_input!(input as WithExpression);

    get_expanded_callsite("histogram", "record", key, labels, op_value)
}

fn get_expanded_registration(
    metric_type: &str,
    key: LitStr,
    desc: LitStr,
    labels: Vec<(LitStr, Expr)>,
) -> TokenStream {
    let register_ident = format_ident!("register_{}", metric_type, span = key.span());
    let insertable_labels = labels
        .into_iter()
        .map(|(k, v)| quote! { metrics::Label::new(#k, #v) });

    let expanded = quote! {
        {
            // Only do this work if there's a recorder installed.
            if let Some(recorder) = metrics::try_recorder() {
                let mlabels = vec![#(#insertable_labels),*];
                recorder.#register_ident((#key, mlabels).into(), Some(#desc));
            }
        }
    };

    debug_tokens(&expanded);

    TokenStream::from(expanded)
}

fn get_expanded_callsite<V>(
    metric_type: &str,
    op_type: &str,
    key: LitStr,
    labels: Vec<(LitStr, Expr)>,
    op_values: V,
) -> TokenStream
where
    V: ToTokens,
{
    let safe_key = make_key_safe(&key);
    let register_ident = format_ident!("register_{}", metric_type, span = key.span());
    let op_ident = format_ident!("{}_{}", op_type, metric_type, span = key.span());

    let use_fast_path = can_use_fast_path(&labels);
    let insertable_labels = labels
        .into_iter()
        .map(|(k, v)| quote! { metrics::Label::new(#k, #v) });

    let expanded = if use_fast_path {
        // We're on the fast path here, so we'll end up registering with the recorder
        // and statically caching the identifier for our metric to speed up any future
        // increment operations.
        let init = format_ident!("METRICS_{}_INIT", safe_key, span = key.span());
        quote! {
            {
                static #init: metrics::OnceIdentifier = metrics::OnceIdentifier::new();

                // Only do this work if there's a recorder installed.
                if let Some(recorder) = metrics::try_recorder() {
                    // Initialize our fast path cached identifier.
                    let id = #init.get_or_init(|| {
                        let mlabels = vec![#(#insertable_labels),*];
                        recorder.#register_ident((#key, mlabels).into(), None)
                    });

                    recorder.#op_ident(id, #op_values);
                }
            }
        }
    } else {
        // We're on the slow path, so basically we register every single time.
        //
        // Recorders are expected to deduplicate any duplicate registrations.
        quote! {
            {
                // Only do this work if there's a recorder installed.
                if let Some(recorder) = metrics::try_recorder() {
                    let mlabels = vec![#(#insertable_labels),*];
                    let id = recorder.#register_ident((#key, mlabels).into(), None);

                    recorder.#op_ident(id, #op_values);
                }
            }
        }
    };

    debug_tokens(&expanded);

    TokenStream::from(expanded)
}

fn make_key_safe(key: &LitStr) -> String {
    let key_str = key.value();
    let safe_chars = key_str.chars().map(|c| {
        if c.is_ascii_alphanumeric() {
            c.to_ascii_uppercase()
        } else {
            '_'
        }
    });
    String::from_iter(safe_chars)
}

fn can_use_fast_path(labels: &[(LitStr, Expr)]) -> bool {
    let mut use_fast_path = true;
    for (_, lvalue) in labels {
        match lvalue {
            Expr::Lit(_) => {}
            _ => {
                use_fast_path = false;
            }
        }
    }
    use_fast_path
}

#[rustversion::nightly]
fn debug_tokens<T: ToTokens>(tokens: &T) {
    if std::env::var_os("METRICS_DEBUG").is_some() {
        let ts = tokens.into_token_stream();
        proc_macro::Span::call_site()
            .note("emitting metrics macro debug output")
            .note(ts.to_string())
            .emit()
    }
}

#[rustversion::not(nightly)]
fn debug_tokens<T: ToTokens>(_tokens: &T) {
    if std::env::var_os("METRICS_DEBUG").is_some() {
        eprintln!("nightly required to output proc macro diagnostics!");
    }
}
