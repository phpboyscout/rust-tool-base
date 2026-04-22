Feature: rtb-error — typed diagnostics and the rendering pipeline
  As a downstream rtb-* crate
  I want a canonical Error enum and a single installable report surface
  So that every error the framework emits renders consistently at the process edge

  Background:
    Given a fresh process with no miette hook installed

  Scenario: S1 — a typed diagnostic renders its code, help, and message
    Given an Error::CommandNotFound built with the name "deploy"
    When I render the diagnostic with the default graphical handler
    Then the rendered output contains the code "rtb::command_not_found"
    And the rendered output contains the help "run `--help` to list available commands"
    And the rendered output contains the message "command not found: deploy"

  Scenario: S2 — a wrapped downstream diagnostic renders transparently
    Given a downstream diagnostic with code "mytool::oops" and help "try turning it off and on again"
    And the downstream diagnostic is boxed into Error::Other
    When I render the wrapped diagnostic with the default graphical handler
    Then the rendered output contains the code "mytool::oops"
    And the rendered output contains the help "try turning it off and on again"
    And the rendered output does not contain the code "rtb::other"

  Scenario: S3 — install_panic_hook leaves the panic machinery functional
    Given I have called rtb_error::hook::install_panic_hook
    When a panic is raised and caught with the message "bang"
    Then catch_unwind observed the panic
    And the panic payload contains "bang"

  Scenario: S4 — install_with_footer appends the footer to every render
    Given I have called rtb_error::hook::install_with_footer with a footer returning "support: slack://#team"
    And an Error::FeatureDisabled built with the feature name "mcp"
    When I render the diagnostic via miette::Report
    Then the rendered output contains "support: slack://#team"

  Scenario: S5 — install_report_handler is idempotent
    Given I have called rtb_error::hook::install_report_handler
    When I call rtb_error::hook::install_report_handler a second time
    Then no panic occurs
    And rendering a diagnostic still succeeds

  Scenario: S6 — FeatureDisabled's help is the canonical rebuild hint
    Given an Error::FeatureDisabled built with the feature name "mcp"
    When I render the diagnostic with the default graphical handler
    Then the rendered output contains the help "rebuild with the appropriate Cargo feature enabled"
