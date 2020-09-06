extern crate proc_macro;

use self::proc_macro::TokenStream;

use proc_macro_hack::proc_macro_hack;
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Error, Parse, ParseStream, Result};
use syn::{parse_macro_input, Expr, LitStr, Token};

enum Key {
    NotScoped(LitStr),
    Scoped(LitStr),
}

enum Labels {
    Existing(Expr),
    Inline(Vec<(LitStr, Option<Expr>)>),
}

struct WithoutExpression {
    key: Key,
    labels: Option<Labels>,
}

struct WithExpression {
    key: Key,
    op_value: Expr,
    labels: Option<Labels>,
}

struct Registration {
    key: Key,
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

        // This may or may not be the start of labels, if the description has been omitted, so
        // we hold on to it until we can make sure nothing else is behind it, or if it's a full
        // fledged set of labels.
        let (description, labels) = if input.peek(Token![,]) && input.peek3(Token![=>]) {
            // We have a ", <something> =>" pattern, which can only be labels, so we have no
            // description.
            let labels = parse_labels(&mut input)?;

            (None, labels)
        } else if input.peek(Token![,]) && input.peek2(LitStr) {
            // We already know we're not working with labels only, and if we have ", <literal
            // string>" then we have to at least have a description, possibly with labels.
            input.parse::<Token![,]>()?;
            let description = input.parse::<LitStr>().ok();
            let labels = parse_labels(&mut input)?;
            (description, labels)
        } else {
            // We might have labels passed as an expression.
            let labels = parse_labels(&mut input)?;
            (None, labels)
        };

        Ok(Registration {
            key,
            description,
            labels,
        })
    }
}

#[proc_macro_hack]
pub fn register_counter(input: TokenStream) -> TokenStream {
    let Registration {
        key,
        description,
        labels,
    } = parse_macro_input!(input as Registration);

    get_expanded_registration("counter", key, description, labels)
}

#[proc_macro_hack]
pub fn register_gauge(input: TokenStream) -> TokenStream {
    let Registration {
        key,
        description,
        labels,
    } = parse_macro_input!(input as Registration);

    get_expanded_registration("gauge", key, description, labels)
}

#[proc_macro_hack]
pub fn register_histogram(input: TokenStream) -> TokenStream {
    let Registration {
        key,
        description,
        labels,
    } = parse_macro_input!(input as Registration);

    get_expanded_registration("histogram", key, description, labels)
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
    description: Option<LitStr>,
    labels: Option<Labels>,
) -> TokenStream {
    let register_ident = format_ident!("register_{}", metric_type);
    let key = key_to_quoted(key, labels);

    let description = match description {
        Some(s) => quote! { Some(#s) },
        None => quote! { None },
    };

    let expanded = quote! {
        {
            // Only do this work if there's a recorder installed.
            if let Some(recorder) = metrics::try_recorder() {
                recorder.#register_ident(#key, #description);
            }
        }
    };

    TokenStream::from(expanded)
}

fn get_expanded_callsite<V>(
    metric_type: &str,
    op_type: &str,
    key: Key,
    labels: Option<Labels>,
    op_values: V,
) -> TokenStream
where
    V: ToTokens,
{
    let register_ident = format_ident!("register_{}", metric_type);
    let op_ident = format_ident!("{}_{}", op_type, metric_type);
    let use_fast_path = can_use_fast_path(&labels);
    let key = key_to_quoted(key, labels);

    let op_values = if metric_type == "histogram" {
        quote! {
            metrics::__into_u64(#op_values)
        }
    } else {
        quote! { #op_values }
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
                        recorder.#register_ident(#key, None)
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
                    let id = recorder.#register_ident(#key, None);
                    recorder.#op_ident(id, #op_values);
                }
            }
        }
    };

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

fn key_to_quoted(key: Key, labels: Option<Labels>) -> proc_macro2::TokenStream {
    let name = match key {
        Key::NotScoped(s) => {
            quote! { #s }
        }
        Key::Scoped(s) => {
            quote! {
                format!("{}.{}", std::module_path!().replace("::", "."), #s)
            }
        }
    };

    match labels {
        None => quote! { metrics::Key::from_name(#name) },
        Some(labels) => match labels {
            Labels::Inline(pairs) => {
                let labels = pairs
                    .into_iter()
                    .map(|(k, v)| match v {
                        Some(v) => quote! { metrics::Label::from_static(#k, #v) },
                        None => quote! { metrics::Label::from_dynamic(#k) },
                    });
                quote! {
                    metrics::Key::from_name_and_labels(#name, vec![#(#labels),*])
                }
            }
            Labels::Existing(e) => {
                quote! {
                    metrics::Key::from_name_and_labels(#name, #e)
                }
            }
        },
    }
}

fn can_use_fast_path(labels: &Option<Labels>) -> bool {
    match labels {
        None => true,
        Some(labels) => match labels {
            Labels::Existing(_) => false,
            Labels::Inline(pairs) => {
                let mut use_fast_path = true;
                for (_, lvalue) in pairs {
                    match lvalue {
                        Some(Expr::Lit(_))=> {},
                        _ => {
                            use_fast_path = false;
                        },
                    }
                }
                use_fast_path
            }
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
    // We check to see if we have the ", key" part, which tells us that we're taking in key/value
    // pairs.  If we don't have that, we check to see if we have a "`, <expr" part, which could us
    // getting handed a labels iterator.  The type checking for `IntoLabels` in `metrics::Recorder`
    // will do the heavy lifting from that point forward.
    if input.peek(Token![,]) && input.peek2(LitStr) {
        let mut labels = Vec::new();
        loop {
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            let lkey: LitStr = input.parse()?;

            if let Ok(_) = input.parse::<Token![=>]>() {
                // Full key/value pair.
                let lvalue: Expr = input.parse()?;
                labels.push((lkey, Some(lvalue))); 
            } else {
                // Dynamic label (unresolved value).
                labels.push((lkey, None));
            }
        }

        return Ok(Some(Labels::Inline(labels)));
    }

    // Has to be an expression otherwise.
    input.parse::<Token![,]>()?;
    let lvalue: Expr = input.parse().map_err(|e| {
        Error::new(
            e.span(),
            "expected label expression, but expression not found",
        )
    })?;
    Ok(Some(Labels::Existing(lvalue)))
}
