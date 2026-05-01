Feature: rtb-cli — application scaffolding and built-in commands
  As a tool author
  I want Application::builder to wire the whole CLI lifecycle in one call
  So that my main() is a one-liner and every tool behaves consistently

  Scenario: S1 — `version` dispatches successfully
    Given a basic Application
    When I dispatch "version"
    Then the result is Ok

  Scenario: S2 — Unknown command surfaces CommandNotFound
    Given a basic Application
    When I dispatch "nope"
    Then the result is an Err mentioning "not_found"

  Scenario: S5 — Disabling Update hides the command
    Given a basic Application with Update feature disabled
    When I dispatch "update"
    Then the result is an Err

  # S6 — retired: the `update` FeatureDisabled stub lived in
  # `rtb-cli` until `rtb-update` v0.1 shipped and took over the
  # `update` command registration. The real update flow's
  # acceptance tests live in `crates/rtb-update/tests/`.

  Scenario: S7 — Init runs a registered initialiser
    Given a basic Application
    When I dispatch "init"
    Then the result is Ok
