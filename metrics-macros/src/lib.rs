extern crate proc_macro;

use self::proc_macro::TokenStream;

use proc_macro_hack::proc_macro_hack;
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Error, Parse, ParseStream, Result};
use syn::{parse_macro_input, Expr, LitStr, Token};

#[cfg(test)]
mod tests;

enum Key {
    NotScoped(LitStr),
    Scoped(LitStr),
}

enum Labels {
    Existing(Expr),
    Inline(Vec<(LitStr, Expr)>),
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

    get_expanded_registration("counter", key, description, labels).into()
}

#[proc_macro_hack]
pub fn register_gauge(input: TokenStream) -> TokenStream {
    let Registration {
        key,
        description,
        labels,
    } = parse_macro_input!(input as Registration);

    get_expanded_registration("gauge", key, description, labels).into()
}

#[proc_macro_hack]
pub fn register_histogram(input: TokenStream) -> TokenStream {
    let Registration {
        key,
        description,
        labels,
    } = parse_macro_input!(input as Registration);

    get_expanded_registration("histogram", key, description, labels).into()
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
    key: Key,
    description: Option<LitStr>,
    labels: Option<Labels>,
) -> proc_macro2::TokenStream {
    let register_ident = format_ident!("register_{}", metric_type);
    let key = prepare_quoted_registration(key, labels);

    let description = match description {
        Some(s) => quote! { Some(#s) },
        None => quote! { None },
    };

    quote! {
        {
            // Only do this work if there's a recorder installed.
            if let Some(recorder) = metrics::try_recorder() {
                recorder.#register_ident(#key, #description);
            }
        }
    }
}

fn get_expanded_callsite<V>(
    metric_type: &str,
    op_type: &str,
    key: Key,
    labels: Option<Labels>,
    op_values: V,
) -> proc_macro2::TokenStream
where
    V: ToTokens,
{
    let (key, dynamic_labels) = prepare_quoted_op(key, labels);

    let op_values = if metric_type == "histogram" {
        quote! {
            metrics::__into_u64(#op_values)
        }
    } else {
        quote! { #op_values }
    };

    match dynamic_labels {
        Some(dynamic_labels) => {
            let op_ident = format_ident!("{}_dynamic_{}", op_type, metric_type);
            quote! {
                {
                    if let Some(recorder) = metrics::try_recorder() {
                        recorder.#op_ident(#key, #op_values, #dynamic_labels);
                    }
                }
            }
        }
        None => {
            let register_ident = format_ident!("register_{}", metric_type);
            let op_ident = format_ident!("{}_{}", op_type, metric_type);
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
        }
    }
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

fn quote_key_name(key: Key) -> proc_macro2::TokenStream {
    match key {
        Key::NotScoped(s) => {
            quote! { #s }
        }
        Key::Scoped(s) => {
            quote! {
                format!("{}.{}", std::module_path!().replace("::", "."), #s)
            }
        }
    }
}

fn prepare_quoted_registration(key: Key, labels: Option<Labels>) -> proc_macro2::TokenStream {
    let name = quote_key_name(key);

    match labels {
        None => quote! { metrics::Key::from_name(#name) },
        Some(labels) => match labels {
            Labels::Inline(pairs) => {
                let labels = pairs
                    .into_iter()
                    .map(|(key, val)| quote! { metrics::Label::new(#key, #val) });
                quote! { metrics::Key::from_name_and_labels(#name, vec![#(#labels),*]) }
            }
            Labels::Existing(e) => quote! { metrics::Key::from_name_and_labels(#name, #e) },
        },
    }
}

fn prepare_quoted_op(
    key: Key,
    labels: Option<Labels>,
) -> (proc_macro2::TokenStream, Option<proc_macro2::TokenStream>) {
    let name = quote_key_name(key);

    match labels {
        None => (quote! { metrics::Key::from_name(#name) }, None),
        Some(labels) => match labels {
            Labels::Inline(pairs) => {
                let mut static_labels = Vec::new();
                let mut dynamic_labels = Vec::new();
                for (key, val) in pairs {
                    let label = quote! { metrics::Label::new(#key, #val) };
                    match val {
                        Expr::Lit(_) => static_labels.push(label),
                        _ => dynamic_labels.push(label),
                    }
                }
                (
                    quote! { metrics::Key::from_name_and_labels(#name, vec![#(#static_labels),*]) },
                    if dynamic_labels.is_empty() {
                        None
                    } else {
                        Some(quote! { vec![#(#dynamic_labels),*] })
                    },
                )
            }
            Labels::Existing(e) => (
                quote! { metrics::Key::from_name(#name) },
                Some(quote! { metrics::IntoLabels::into_labels(#e) }),
            ),
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
            let lkey: LitStr = input.parse()?;
            input.parse::<Token![=>]>()?;
            let lvalue: Expr = input.parse()?;

            labels.push((lkey, lvalue));
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
