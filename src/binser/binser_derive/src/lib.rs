extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{Data, DeriveInput, Error, Fields, GenericParam, Generics, Ident, Index, parse_macro_input, parse_quote, spanned::Spanned};

#[proc_macro_derive(FromLe)]
pub fn from_le(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Add a bound `T: FromLe` to every type parameter T.
    let generics = from_le_add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Generate the method doing the conversion.
    let from_le = gen_from_le(name, &input.data);

    let expanded = quote! {
        impl #impl_generics ::binser::FromLe for #name #ty_generics #where_clause {
	    #from_le
        }
    };

    expanded.into()
}

fn from_le_add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
	if let GenericParam::Type(ref mut type_param) = *param {
	    type_param.bounds.push(parse_quote!(::binser::FromLe));
	}
    }
    generics
}

fn gen_from_le(name: &Ident, data: &Data) -> TokenStream {
    match data {
        Data::Struct(ref data) => {
            match data.fields {
                Fields::Named(ref fields) => {
                    let converted_fields = fields.named.iter().map(|f| {
                        let name = &f.ident;
                        quote_spanned! {f.span()=>
                            #name: ::binser::FromLe::from_le(x.#name)
                        }
                    });
                    quote! {
                        fn from_le(x: #name) -> #name {
                            #name {
                                #(#converted_fields,)*
                            }
                        }
                    }
                },
                Fields::Unnamed(ref fields) => {
                    let converted_fields = fields.unnamed.iter().enumerate().map(|(i, f)| {
                        let index = Index::from(i);
                        quote_spanned! {f.span()=>
                            ::binser::FromLe::from_le(x.#index)
                        }
                    });
                    quote! {
                        fn from_le(x: #name) -> #name {
                            #name {
                                #(#converted_fields,)*
                            }
                        }
                    }

                },
                Fields::Unit => {
                    Error::new(Span::call_site(), "Empty structs cannot be automatically made FromLe").to_compile_error()
                },
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            Error::new(Span::call_site(), "enum and union types cannot be automatically made FromLe").to_compile_error()
        },
    }
}

#[proc_macro_derive(IntoLe)]
pub fn into_le(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Add a bound `T: IntoLe` to every type parameter T.
    let generics = into_le_add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Generate the method doing the conversion.
    let into_le = gen_into_le(name, &input.data);

    let expanded = quote! {
        impl #impl_generics ::binser::IntoLe for #name #ty_generics #where_clause {
	    #into_le
        }
    };

    expanded.into()
}

fn into_le_add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
	if let GenericParam::Type(ref mut type_param) = *param {
	    type_param.bounds.push(parse_quote!(::binser::IntoLe));
	}
    }
    generics
}

fn gen_into_le(name: &Ident, data: &Data) -> TokenStream {
    match data {
        Data::Struct(ref data) => {
            match data.fields {
                Fields::Named(ref fields) => {
                    let converted_fields = fields.named.iter().map(|f| {
                        let name = &f.ident;
                        quote_spanned! {f.span()=>
                            #name: self.#name.into_le()
                        }
                    });
                    quote! {
                        fn into_le(self) -> #name {
                            #name {
                                #(#converted_fields,)*
                            }
                        }
                    }
                },
                Fields::Unnamed(ref fields) => {
                    let converted_fields = fields.unnamed.iter().enumerate().map(|(i, f)| {
                        let index = Index::from(i);
                        quote_spanned! {f.span()=>
                            self.#index.into_le()
                        }
                    });
                    quote! {
                        fn into_le(self) -> #name {
                            #name {
                                #(#converted_fields,)*
                            }
                        }
                    }

                },
                Fields::Unit => {
                    Error::new(Span::call_site(), "Empty structs cannot be automatically made IntoLe").to_compile_error()
                },
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            Error::new(Span::call_site(), "enum and union types cannot be automatically made IntoLe").to_compile_error()
        },
    }
}

#[proc_macro_derive(ValidAsBytes)]
pub fn valid_as_bytes(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl ValidAsBytes for #name {
        }
    };

    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(Validate)]
pub fn validate(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl Validate for #name {
        }
    };

    proc_macro::TokenStream::from(expanded)
}
