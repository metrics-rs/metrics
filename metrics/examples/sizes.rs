//! This example is purely for development.
use metrics::{Key, KeyData, Label, NameParts, SharedString};
use std::borrow::Cow;

fn main() {
    println!("KeyData: {} bytes", std::mem::size_of::<KeyData>());
    println!("Key: {} bytes", std::mem::size_of::<Key>());
    println!("NameParts: {} bytes", std::mem::size_of::<NameParts>());
    println!("Label: {} bytes", std::mem::size_of::<Label>());
    println!(
        "Cow<'static, [Label]>: {} bytes",
        std::mem::size_of::<Cow<'static, [Label]>>()
    );
    println!(
        "Vec<SharedString>: {} bytes",
        std::mem::size_of::<Vec<SharedString>>()
    );
    println!(
        "[Option<SharedString>; 2]: {} bytes",
        std::mem::size_of::<[Option<SharedString>; 2]>()
    );
}
