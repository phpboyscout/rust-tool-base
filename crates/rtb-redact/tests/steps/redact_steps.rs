//! Step implementations for the redact feature.

use cucumber::{given, then, when};

use super::RedactWorld;

#[given(regex = r#"^the input is "(.*)"$"#)]
fn given_input(world: &mut RedactWorld, input: String) {
    world.input = input;
}

#[given("the input is a multi-line PEM log")]
fn given_pem_input(world: &mut RedactWorld) {
    world.input = String::from(
        "routine log entry\n\
         -----BEGIN RSA PRIVATE KEY-----\n\
         MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7VJTUt9Us8cKj\n\
         MZngKj9Y4oEZ9Yyo8D0lfMfPcE0yXBX3vHvwvqjjHGmTIabsoklBhuBXUoMAwawI\n\
         -----END RSA PRIVATE KEY-----\n\
         another routine entry",
    );
}

#[when("I redact the string")]
fn when_redact(world: &mut RedactWorld) {
    world.output = rtb_redact::string(&world.input).into_owned();
}

#[then(regex = r#"^the output is "(.*)"$"#)]
fn then_output_is(world: &mut RedactWorld, expected: String) {
    assert_eq!(world.output, expected, "got: {}", world.output);
}

#[then(regex = r#"^the output contains "(.*)"$"#)]
fn then_output_contains(world: &mut RedactWorld, needle: String) {
    assert!(world.output.contains(&needle), "expected {needle:?} in output: {}", world.output);
}

#[then(regex = r#"^the output does not contain "(.*)"$"#)]
fn then_output_does_not_contain(world: &mut RedactWorld, needle: String) {
    assert!(!world.output.contains(&needle), "unexpected {needle:?} in output: {}", world.output);
}
