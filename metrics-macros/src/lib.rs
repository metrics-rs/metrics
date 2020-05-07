extern crate proc_macro;

use self::proc_macro::TokenStream;

use proc_macro_hack::proc_macro_hack;
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Parse, ParseStream, Result};
use syn::{parse_macro_input, Expr, LitStr, Token};

enum Key {
    NotScoped(LitStr),
    Scoped(LitStr),
}

struct WithoutExpression {
    key: Key,
    labels: Vec<(LitStr, Expr)>,
}

struct WithExpression {
    key: Key,
    op_value: Expr,
    labels: Vec<(LitStr, Expr)>,
}

struct Registration {
    key: Key,
    desc: LitStr,
    labels: Vec<(LitStr, Expr)>,
}

impl Parse for WithoutExpression {
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = read_key(&mut input)?;

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
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = read_key(&mut input)?;

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
    fn parse(mut input: ParseStream) -> Result<Self> {
        let key = read_key(&mut input)?;

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
    key: Key,
    desc: LitStr,
    labels: Vec<(LitStr, Expr)>,
) -> TokenStream {
    let register_ident = format_ident!("register_{}", metric_type);
    let key = match key {
        Key::NotScoped(s) => {
            quote! { #s }
        }
        Key::Scoped(s) => {
            quote! {
                format!("{}.{}", std::module_path!().replace("::", "."), #s)
            }
        }
    };
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
    key: Key,
    labels: Vec<(LitStr, Expr)>,
    op_values: V,
) -> TokenStream
where
    V: ToTokens,
{
    let register_ident = format_ident!("register_{}", metric_type);
    let op_ident = format_ident!("{}_{}", op_type, metric_type);
    let key = match key {
        Key::NotScoped(s) => {
            quote! { #s }
        }
        Key::Scoped(s) => {
            quote! {
                format!("{}.{}", std::module_path!().replace("::", "."), #s)
            }
        }
    };

    let use_fast_path = can_use_fast_path(&labels);
    let composite_key = if labels.is_empty() {
        quote! { #key.into() }
    } else {
        let insertable_labels = labels
            .into_iter()
            .map(|(k, v)| quote! { metrics::Label::new(#k, #v) });
        quote! { (#key, vec![#(#insertable_labels),*]).into() }
    };

    let expanded = if use_fast_path {
        // We're on the fast path here, so we'll end up registering with the recorder
        // and statically caching the identifier for our metric to speed up any future
        // increment operations.
        quote! {
            {
                static METRICS_INIT: metrics::OnceIdentifier = metrics::OnceIdentifier::new();

                // Only do this work if there's a recorder installed.
                if let Some(recorder) = metrics::try_recorder() {
                    // Initialize our fast path cached identifier.
                    let id = METRICS_INIT.get_or_init(|| {
                        recorder.#register_ident(#composite_key, None)
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
                    let id = recorder.#register_ident(#composite_key, None);

                    recorder.#op_ident(id, #op_values);
                }
            }
        }
    };

    debug_tokens(&expanded);

    TokenStream::from(expanded)
}

fn read_key(input: &mut ParseStream) -> Result<Key> {
    if let Ok(_) = input.parse::<Token![<]>() {
        let s = input.parse::<LitStr>()?;
        input.parse::<Token![>]>()?;
        Ok(Key::Scoped(s))
    } else {
        let s = input.parse::<LitStr>()?;
        Ok(Key::NotScoped(s))
    }
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
