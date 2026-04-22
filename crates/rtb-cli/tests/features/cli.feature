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

  Scenario: S6 — Update placeholder returns FeatureDisabled
    Given a basic Application
    When I dispatch "update"
    Then the result is an Err mentioning "not compiled in"

  Scenario: S7 — Init runs a registered initialiser
    Given a basic Application
    When I dispatch "init"
    Then the result is Ok
