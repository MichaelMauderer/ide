//! This crate defines a custom derive macro `Iterator`. Should not be used
//! directly, but only through `shapely` crate, as it provides utilities
//! necessary for the generated code to compile.

#![warn(missing_docs)]
#![warn(trivial_casts)]
#![warn(trivial_numeric_casts)]
#![warn(unused_import_braces)]
#![warn(unused_qualifications)]
#![warn(unsafe_code)]
#![warn(missing_copy_implementations)]
#![warn(missing_debug_implementations)]

extern crate proc_macro;

mod derive_iterator;
mod overlappable;

mod prelude {
    pub use enso_prelude::*;

    pub use macro_utils::repr;
    pub use proc_macro2::Span;
    pub use proc_macro2::TokenStream;
    pub use quote::quote;
}

use crate::derive_iterator::IsMut;

/// For `struct Foo<T>` or `enum Foo<T>` provides:
/// * `IntoIterator` implementations for `&'t Foo<T>`, `iter` and `into_iter`
/// methods.
///
/// The iterators will:
/// * for structs: go over each field that declared type is same as the
///   struct's last type parameter.
/// * enums: delegate to current constructor's nested value's iterator.
///
/// Enums are required to use only a single element tuple-like variant. This
/// limitation should be lifted in the future.
///
/// Any dependent type stored in struct, tuple or wrapped in enum should have
/// dependency only in its last type parameter. All dependent types that are not
/// tuples nor directly the yielded type, are required to provide `iter` method
/// that returns a compatible iterator (possible also derived).
///
/// Caller must have the following features enabled:
/// ```
/// #![feature(generators)]
/// #![feature(type_alias_impl_trait)]
/// ```
///
/// When used on type that takes no type parameters, like `struct Foo`, does
/// nothing but yields no errors.
#[proc_macro_derive(Iterator)]
pub fn derive_iterator
(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_iterator::derive(input,IsMut::Immutable)
}

/// Same as `derive(Iterator)` but generates mutable iterator.
///
/// It is separate, as some types allow deriving immutable iterator but ont the
/// mutable one.
#[proc_macro_derive(IteratorMut)]
pub fn derive_iterator_mut
(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_iterator::derive(input,IsMut::Mutable)
}

#[allow(missing_docs)]
#[proc_macro_attribute]
pub fn overlappable
( attrs : proc_macro::TokenStream
  , input : proc_macro::TokenStream
) -> proc_macro::TokenStream {
    overlappable::overlappable(attrs,input)
}

