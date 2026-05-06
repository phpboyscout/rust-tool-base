//! `CredentialBearing` trait tests — covers spec T24 (blanket impl
//! for `()`) and the canonical "5-line per-tool impl" pattern.

#![allow(missing_docs)]

use rtb_credentials::{CredentialBearing, CredentialRef};

// -- T24 — `()` impl returns empty Vec ------------------------------

#[test]
fn t24_unit_blanket_impl_yields_empty_vec() {
    let unit = ();
    let creds = unit.credentials();
    assert!(creds.is_empty(), "() impl must yield no credentials");
}

// -- Canonical 5-line per-tool impl works ---------------------------

#[derive(Default)]
struct MyConfig {
    anthropic: Section,
    github: Section,
}

#[derive(Default)]
struct Section {
    cred: CredentialRef,
}

impl CredentialBearing for MyConfig {
    fn credentials(&self) -> Vec<(&'static str, &CredentialRef)> {
        vec![("anthropic", &self.anthropic.cred), ("github", &self.github.cred)]
    }
}

#[test]
fn custom_impl_yields_named_refs() {
    let cfg = MyConfig::default();
    let creds = cfg.credentials();
    assert_eq!(creds.len(), 2, "two declared credentials must be enumerated");
    let names: Vec<&'static str> = creds.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, vec!["anthropic", "github"]);
}

// -- The trait must be `dyn`-compatible (object-safe) -----------------
//
// Slice 2's App integration stores a `dyn CredentialBearing`-erased
// provider, so the trait must remain object-safe. This test will
// fail to compile if anyone adds a generic method, `Self`-by-value
// receiver, or other object-unsafety.

#[test]
fn trait_is_object_safe() {
    let cfg: Box<dyn CredentialBearing> = Box::new(MyConfig::default());
    assert_eq!(cfg.credentials().len(), 2);

    let unit: Box<dyn CredentialBearing> = Box::new(());
    assert!(unit.credentials().is_empty());
}
