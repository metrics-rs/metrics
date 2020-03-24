extern crate proc_macro;

use self::proc_macro::TokenStream;

use proc_macro_hack::proc_macro_hack;
use quote::{quote, format_ident, ToTokens};
use syn::parse::{Parse, ParseStream, Result};
use syn::{parse_macro_input, Expr, Token, LitStr};

struct Increment {
    key: LitStr,
    labels: Vec<(LitStr, Expr)>,
}

impl Parse for Increment {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: LitStr = input.parse()?;

        let mut labels = Vec::new();
        loop {
            if input.is_empty() { break }
            input.parse::<Token![,]>()?;
            let lkey: LitStr = input.parse()?;
            input.parse::<Token![=>]>()?;
            let lvalue: Expr = input.parse()?;

            labels.push((lkey, lvalue));
        }
        Ok(Increment { key, labels })
    }
}

#[proc_macro_hack]
pub fn increment(input: TokenStream) -> TokenStream {
    let Increment {
        key,
        labels,
    } = parse_macro_input!(input as Increment);

    let op_value = quote! { 1 };

    get_expanded_callsite(key, labels, "counter", "increment", op_value)
}

fn get_expanded_callsite<V>(key: LitStr, labels: Vec<(LitStr, Expr)>, metric_type: &str, op_type: &str, op_values: V) -> TokenStream
where
    V: ToTokens,
{
    let register_ident = format_ident!("register_{}", metric_type, span = key.span());
    let op_ident = format_ident!("{}_{}", op_type, metric_type, span = key.span());

    let use_fast_path = can_use_fast_path(&labels);
    let insertable_labels = labels.into_iter()
        .map(|(k, v)| quote! { metrics::Label::new(#k, #v) });

    let expanded = if use_fast_path {
        // We're on the fast path here, so we'll end up registering with the recorder
        // and statically caching the identifier for our metric to speed up any future
        // increment operations.
        let init = format_ident!("METRICS_{}_INIT", key.value().to_uppercase(), span = key.span());
        let id = format_ident!("METRICS_{}_ID", key.value().to_uppercase(), span = key.span());
        quote! {
            {
                static #init: std::sync::Once = std::sync::Once::new();
                static mut #id: std::cell::UnsafeCell<metrics::Identifier> = std::cell::UnsafeCell::new(metrics::Identifier::zeroed());

                // Only do this work if there's a recorder installed.
                if let Some(recorder) = metrics::try_recorder() {
                    // Initialize our fast path cached identifier.
                    #init.call_once(|| {
                        let mlabels = vec![#(#insertable_labels),*];
                        let id = recorder.#register_ident((#key, mlabels).into());
                        unsafe { (*#id.get()) = id; }
                    });

                    let lid = unsafe { &*#id.get() };
                    recorder.#op_ident(lid, #op_values);
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
                    let id = recorder.#register_ident((#key, mlabels).into());

                    recorder.#op_ident(id, #op_values);
                }
            }
        }
    };

    eprintln!("tokens: {}", expanded);

    TokenStream::from(expanded)
}

fn can_use_fast_path(labels: &Vec<(LitStr, Expr)>) -> bool {
    let mut use_fast_path = true;
    for (_, lvalue) in labels {
        match lvalue {
            Expr::Lit(_) => {},
            _ => {
                use_fast_path = false;
            },
        }
    }
    use_fast_path
}
